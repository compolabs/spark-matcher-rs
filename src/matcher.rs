use crate::common::*;
use ::log::{debug, error, info};
use anyhow::{Context, Result};
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use orderbook::{constants::RPC, orderbook_utils::Orderbook};
use reqwest::Client;
use std::{str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;
use tokio::try_join;

#[derive(Eq, PartialEq, Clone)]
pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    orderbook: Arc<Mutex<Orderbook>>,
    client: Client,
    initialized: bool,
    status: Status,
}

impl SparkMatcher {
    pub async fn new() -> Result<Self> {
        let provider = Provider::connect(RPC).await?;
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );
        let client = Client::new();

        debug!("Setup SparkMatcher correctly.");
        Ok(Self {
            orderbook: Arc::new(Mutex::new(Orderbook::new(&wallet, &contract_id).await)),
            client,
            initialized: true,
            status: Status::Chill,
        })
    }

    pub async fn init() -> Result<Arc<Mutex<Self>>> {
        Ok(Arc::new(Mutex::new(SparkMatcher::new().await?)))
    }

    pub async fn run(&mut self) {
        info!("Matcher running...");
        self.process_next().await;
    }

    async fn process_next(&mut self) {
        loop {
            if !self.initialized {
                continue;
            }

            if self.status == Status::Active {
                continue;
            }

            self.status = Status::Active;

            let start_time = Instant::now();
            match self.do_match().await {
                Ok(_) => (),
                Err(e) => {
                    error!("An error occurred while matching: `{}`", e);
                }
            }
            let duration = start_time.elapsed();
            info!("Время выполнения do_match: {:?}", duration);

            self.status = Status::Chill;
        }
    }

    async fn do_match(&mut self) -> Result<()> {
        let fetch_start_time = Instant::now();

        let (mut sell_orders, mut buy_orders) = try_join!(
            fetch_orders_from_indexer(&self.client, OrderType::Sell),
            fetch_orders_from_indexer(&self.client, OrderType::Buy)
        )
        .context("Failed to fetch orders")?;

        let fetch_duration = fetch_start_time.elapsed();
        info!("Время выполнения fetch_orders: {:?}", fetch_duration);

        let match_pairs_start_time = Instant::now();
        let mut match_pairs: Vec<(String, String)> = vec![];
        let mut sell_index = 0;
        let mut buy_index = 0;

        while sell_index < sell_orders.len() && buy_index < buy_orders.len() {
            let sell_order = sell_orders.get_mut(sell_index).unwrap();
            let buy_order = buy_orders.get_mut(buy_index).unwrap();

            // Преобразование строк в числа
            let (sell_size, sell_price, buy_size, buy_price) = (
                sell_order
                    .base_size
                    .parse::<i128>()
                    .context("Invalid sell order size")?,
                sell_order
                    .base_price
                    .parse::<i128>()
                    .context("Invalid sell order price")?,
                buy_order
                    .base_size
                    .parse::<i128>()
                    .context("Invalid buy order size")?,
                buy_order
                    .base_price
                    .parse::<i128>()
                    .context("Invalid buy order price")?,
            );

            debug!(
                "Matching sell order: {:?}, buy order: {:?}",
                sell_order, buy_order
            );

            // Проверяем, могут ли ордера быть совпавшими
            if self.match_conditions_met(
                sell_price, buy_price, sell_size, buy_size, sell_order, buy_order,
            ) {
                let amount = sell_size.abs().min(buy_size);
                sell_order.base_size = (sell_size + amount).to_string();
                buy_order.base_size = (buy_size - amount).to_string();

                match_pairs.push((sell_order.id.clone(), buy_order.id.clone()));
            } else {
                // Увеличиваем индексы для следующих проверок
                if sell_price > buy_price {
                    buy_index += 1;
                } else {
                    sell_index += 1;
                }
            }
        }

        let match_pairs_duration = match_pairs_start_time.elapsed();
        info!(
            "Время выполнения сопоставления ордеров: {:?}",
            match_pairs_duration
        );

        info!("match_pairs = {:?}", match_pairs.len());

        // Параллельная обработка match_pairs
        let orderbook = self.orderbook.clone();
        let client = self.client.clone();
        let match_tasks: Vec<_> = match_pairs
            .into_iter()
            .map(|pair| {
                let orderbook = orderbook.clone();
                let client = client.clone();
                spawn_blocking(move || {
                    let matcher = SparkMatcher {
                        orderbook,
                        client,
                        initialized: true,
                        status: Status::Active,
                    };
                    tokio::runtime::Handle::current()
                        .block_on(async move { matcher.match_pairs(vec![pair]).await })
                })
            })
            .collect();

        for task in match_tasks {
            task.await??;
        }

        Ok(())
    }

    fn match_conditions_met(
        &self,
        sell_price: i128,
        buy_price: i128,
        sell_size: i128,
        buy_size: i128,
        sell_order: &SpotOrder,
        buy_order: &SpotOrder,
    ) -> bool {
        sell_order.base_token == buy_order.base_token
            && sell_size < 0
            && buy_size > 0
            && sell_price <= buy_price
    }

    async fn match_pairs(&self, match_pairs: Vec<(String, String)>) -> Result<()> {
        if match_pairs.is_empty() {
            return Ok(());
        }

        let match_pairs_start_time = Instant::now();

        match self
            .orderbook
            .lock()
            .await
            .match_in_pairs(
                match_pairs
                    .iter()
                    .map(|(s, b)| {
                        (
                            Bits256::from_hex_str(s).unwrap(),
                            Bits256::from_hex_str(b).unwrap(),
                        )
                    })
                    .collect(),
            )
            .await
        {
            Ok(_) => {
                info!(
                    "✅ Matched these pairs (sell, buy): => `{:#?}`!\n",
                    &match_pairs
                );
                let match_pairs_duration = match_pairs_start_time.elapsed();
                info!("Время выполнения match_pairs: {:?}", match_pairs_duration);
            }
            Err(e) => {
                error!("matching error `{}`", e);
                error!(
                    "Tried to match these pairs, but failed: (sell, buy) => `{:#?}`.",
                    &match_pairs
                );
                let match_pairs_duration = match_pairs_start_time.elapsed();
                info!(
                    "Время выполнения match_pairs (c ошибкой): {:?}",
                    match_pairs_duration
                );
                return Err(e.into());
            }
        }
        Ok(())
    }
}

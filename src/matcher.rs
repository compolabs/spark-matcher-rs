use ::log::{debug, error, info, warn};
use anyhow::{Context, Result};
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use orderbook::{constants::RPC, orderbook_utils::Orderbook};
use reqwest::Client;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio::try_join;

use crate::common::*;

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    orderbook: Arc<Mutex<Orderbook>>,
    initialized: bool,
    status: Status,
    ignore_list: Vec<String>,
    client: Client,
}

impl SparkMatcher {
    pub async fn new(client: Client) -> Result<Self> {
        let start = Instant::now();
        let provider = Provider::connect(RPC).await?;
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );

        debug!("Setup SparkMatcher correctly.");
        let duration = start.elapsed();
        info!("SparkMatcher::new executed in {:?}", duration);

        Ok(Self {
            orderbook: Arc::new(Mutex::new(Orderbook::new(&wallet, &contract_id).await)),
            initialized: true,
            status: Status::Chill,
            ignore_list: vec![],
            client,
        })
    }

    pub async fn init(client: Client) -> Result<Arc<Mutex<Self>>> {
        let start = Instant::now();
        let result = Arc::new(Mutex::new(SparkMatcher::new(client).await?));
        let duration = start.elapsed();
        info!("SparkMatcher::init executed in {:?}", duration);
        Ok(result)
    }

    pub async fn run(&mut self) {
        info!("Matcher launched, running...");
        let start = Instant::now();
        self.process_next().await;
        let duration = start.elapsed();
        info!("SparkMatcher::run executed in {:?}", duration);
    }

    async fn process_next(&mut self) {
        loop {
            if !self.initialized {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            if self.status == Status::Active {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            self.status = Status::Active;

            let start = Instant::now();
            match self.do_match().await {
                Ok(_) => (),
                Err(e) => {
                    error!("An error occurred while matching: `{}`", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            let duration = start.elapsed();
            info!("SparkMatcher::process_next executed in {:?}", duration);

            self.ignore_list.clear();
            self.status = Status::Chill;
        }
    }

    async fn do_match(&mut self) -> Result<()> {
        let start = Instant::now();
        let (sell_orders_result, buy_orders_result) = tokio::join!(
            fetch_orders_from_indexer(OrderType::Sell, &self.client),
            fetch_orders_from_indexer(OrderType::Buy, &self.client)
        );

        let mut sell_orders = sell_orders_result.context("Failed to fetch sell orders")?;
        let mut buy_orders = buy_orders_result.context("Failed to fetch buy orders")?;

        debug!(
            "Sell orders: {:?}",
            sell_orders.iter().take(5).collect::<Vec<_>>()
        );
        debug!(
            "Buy orders: {:?}",
            buy_orders.iter().take(5).collect::<Vec<_>>()
        );

        let match_pairs_start = Instant::now();
        let match_pairs = self
            .find_matching_pairs(&mut sell_orders, &mut buy_orders)
            .await?;
        let match_pairs_duration = match_pairs_start.elapsed();
        info!(
            "SparkMatcher::find_matching_pairs executed in {:?}",
            match_pairs_duration
        );

        if !match_pairs.is_empty() {
            let match_pairs_exec_start = Instant::now();
            if let Err(_e) = self.match_pairs(match_pairs.clone()).await {
                for pair in match_pairs {
                    if let Err(e) = self.match_single_pair(pair).await {
                        error!("Failed to match single pair: {}", e);
                    }
                }
            }
            let match_pairs_exec_duration = match_pairs_exec_start.elapsed();
            info!(
                "SparkMatcher::match_pairs executed in {:?}",
                match_pairs_exec_duration
            );
        }
        let duration = start.elapsed();
        info!("SparkMatcher::do_match executed in {:?}", duration);
        Ok(())
    }

    async fn find_matching_pairs(
        &mut self,
        sell_orders: &mut Vec<SpotOrder>,
        buy_orders: &mut Vec<SpotOrder>,
    ) -> Result<Vec<(String, String)>> {
        let start = Instant::now();
        let mut match_pairs: Vec<(String, String)> = vec![];
        let mut sell_index = 0;
        let mut buy_index = 0;

        while sell_index < sell_orders.len() && buy_index < buy_orders.len() {
            let sell_order = sell_orders.get_mut(sell_index).unwrap();
            let buy_order = buy_orders.get_mut(buy_index).unwrap();

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

            if sell_size == 0 {
                info!("Sell order with size 0 found: {:?}", sell_order);
                sell_index += 1;
                continue;
            }
            if buy_size == 0 {
                info!("Buy order with size 0 found: {:?}", buy_order);
                buy_index += 1;
                continue;
            }

            if self.match_conditions_met(
                sell_price, buy_price, sell_size, buy_size, sell_order, buy_order,
            ) {
                let is_phantom_order_start = Instant::now();
                if self.is_phantom_order(sell_order, buy_order).await? {
                    sell_order.base_size = "0".to_string();
                    buy_order.base_size = "0".to_string();
                    let is_phantom_order_duration = is_phantom_order_start.elapsed();
                    info!(
                        "SparkMatcher::is_phantom_order executed in {:?}",
                        is_phantom_order_duration
                    );
                    continue;
                }
                let is_phantom_order_duration = is_phantom_order_start.elapsed();
                info!(
                    "SparkMatcher::is_phantom_order executed in {:?}",
                    is_phantom_order_duration
                );

                let amount = sell_size.abs().min(buy_size);
                sell_order.base_size = (sell_size + amount).to_string();
                buy_order.base_size = (buy_size - amount).to_string();

                match_pairs.push((sell_order.id.clone(), buy_order.id.clone()));

                if sell_order.base_size == "0" {
                    sell_index += 1;
                }

                if buy_order.base_size == "0" {
                    buy_index += 1;
                }
            } else {
                if sell_price > buy_price {
                    buy_index += 1;
                } else {
                    sell_index += 1;
                }
            }
        }

        let duration = start.elapsed();
        info!(
            "SparkMatcher::find_matching_pairs executed in {:?}",
            duration
        );
        Ok(match_pairs)
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
        !self.ignore_list.contains(&sell_order.id)
            && !self.ignore_list.contains(&buy_order.id)
            && sell_order.base_token == buy_order.base_token
            && sell_size < 0
            && buy_size > 0
            && sell_price <= buy_price
    }

    async fn is_phantom_order(
        &mut self,
        sell_order: &SpotOrder,
        buy_order: &SpotOrder,
    ) -> Result<bool> {
        let start = Instant::now();
        let sell_id = Bits256::from_hex_str(&sell_order.id)?;
        let buy_id = Bits256::from_hex_str(&buy_order.id)?;

        let (sell_is_phantom, buy_is_phantom) = try_join!(
            self.is_order_phantom(&sell_id),
            self.is_order_phantom(&buy_id)
        )?;

        if sell_is_phantom {
            warn!("👽 Phantom order detected: sell: `{}`.", &sell_order.id);
            self.ignore_list.push(sell_order.id.clone())
        }

        if buy_is_phantom {
            warn!("👽 Phantom order detected: buy: `{}`.", &buy_order.id);
            self.ignore_list.push(buy_order.id.clone())
        }

        let duration = start.elapsed();
        info!("SparkMatcher::is_phantom_order executed in {:?}", duration);

        Ok(sell_is_phantom || buy_is_phantom)
    }

    async fn is_order_phantom(&self, id: &Bits256) -> Result<bool> {
        let start = Instant::now();
        let orderbook = self.orderbook.lock().await;
        let order = orderbook.order_by_id(id).await?;
        let duration = start.elapsed();
        info!("SparkMatcher::is_order_phantom executed in {:?}", duration);
        Ok(order.value.is_none())
    }

    async fn match_pairs(&self, match_pairs: Vec<(String, String)>) -> Result<()> {
        if match_pairs.is_empty() {
            return Ok(());
        }

        let start = Instant::now();

        for (sell, buy) in match_pairs {
            let sell_id = Bits256::from_hex_str(&sell).unwrap();
            let buy_id = Bits256::from_hex_str(&buy).unwrap();

            let orderbook = self.orderbook.lock().await;
            if let Err(e) = orderbook.match_in_pairs(vec![(sell_id, buy_id)]).await {
                error!("Failed to match pair: {:?}, Error: {:?}", (sell, buy), e);
            }
        }

        let duration = start.elapsed();
        info!("SparkMatcher::match_pairs executed in {:?}", duration);
        Ok(())
    }

    async fn match_single_pair(&self, pair: (String, String)) -> Result<()> {
        let start = Instant::now();
        let (sell, buy) = pair;

        let orderbook = self.orderbook.lock().await;
        orderbook
            .match_in_pairs(vec![(
                Bits256::from_hex_str(&sell).unwrap(),
                Bits256::from_hex_str(&buy).unwrap(),
            )])
            .await?;

        info!(
            "✅ Matched single pair (sell, buy): => `({}, {})`\n",
            sell, buy
        );
        let duration = start.elapsed();
        info!("SparkMatcher::match_single_pair executed in {:?}", duration);
        Ok(())
    }
}

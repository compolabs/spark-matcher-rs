use ::log::{debug, error, info, warn};
use anyhow::Result;
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use tokio::sync::Mutex;
use orderbook::{constants::RPC, orderbook_utils::Orderbook};
use std::{str::FromStr, sync::Arc, time::Duration};

use crate::common::*;

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    orderbook: Orderbook,
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

        debug!("Setup SparkMatcher correctly.");
        Ok(Self {
            orderbook: Orderbook::new(&wallet, &contract_id).await,
            initialized: true,
            status: Status::Chill,
        })
    }

    pub async fn init() -> Result<Arc<Mutex<Self>>> {
        Ok(Arc::new(Mutex::new(SparkMatcher::new().await?)))
    }

    pub async fn run(&mut self) {
        self.process_next().await;
    }

    async fn process_next(&mut self) {
        loop {
            if !self.initialized {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            }

            if self.status == Status::Active {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            }

            self.status = Status::Active;

            match self.do_match().await {
                Ok(_) => (),
                Err(e) => {
                    error!("An error occurred while matching: `{}`", e);
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }

            self.status = Status::Chill;
        }
    }

    async fn do_match(&mut self) -> Result<()> {
        let (mut sell_orders, mut buy_orders) = tokio::try_join!(
            fetch_orders_from_indexer(OrderType::Sell),
            fetch_orders_from_indexer(OrderType::Buy)
        )?;
        debug!(
            "Sell orders for this match: `{:#?}`\n\nBuy orders for this match: `{:#?}`\n\n",
            &sell_orders, &buy_orders
        );

        let mut match_pairs: Vec<(String, String)> = vec![];
        let mut sell_index = 0;
        let mut buy_index = 0;

        while sell_index < sell_orders.len() && buy_index < buy_orders.len() {
            let sell_order = sell_orders.get_mut(sell_index).unwrap();
            let buy_order = buy_orders.get_mut(buy_index).unwrap();

            let (sell_size, sell_price, buy_size, buy_price) = (
                sell_order.base_size.parse::<i128>()?,
                sell_order.base_price.parse::<i128>()?,
                buy_order.base_size.parse::<i128>()?,
                buy_order.base_price.parse::<i128>()?,
            );

            if sell_size == 0 {
                sell_index += 1;
                continue;
            }
            if buy_size == 0 {
                buy_index += 1;
                continue;
            }

            if self.match_conditions_met(sell_price, buy_price, sell_size, buy_size, sell_order, buy_order) {
                if self.is_phantom_order(sell_order, buy_order).await? {
                    sell_order.base_size = "0".to_string();
                    buy_order.base_size = "0".to_string();
                    continue;
                }

                let amount = sell_size.abs().min(buy_size);
                sell_order.base_size = (sell_size + amount).to_string();
                buy_order.base_size = (buy_size - amount).to_string();

                match_pairs.push((sell_order.order_id.clone(), buy_order.order_id.clone()));
            } else {
                self.match_pairs(match_pairs.clone()).await?;
                break;
            }
        }
        self.match_pairs(match_pairs).await?;
        Ok(())
    }

    fn match_conditions_met(
        &self,
        sell_price: i128,
        buy_price: i128,
        sell_size: i128,
        buy_size: i128,
        sell_order: &IndexerOrder,
        buy_order: &IndexerOrder,
    ) -> bool {
        sell_price <= buy_price
            && sell_size < 0
            && buy_size > 0
            && sell_order.base_token == buy_order.base_token
    }

    async fn is_phantom_order(&self, sell_order: &IndexerOrder, buy_order: &IndexerOrder) -> Result<bool> {
        let sell_id = Bits256::from_hex_str(&sell_order.order_id)?;
        let buy_id = Bits256::from_hex_str(&buy_order.order_id)?;

        let sell_is_phantom = self.is_order_phantom(&sell_id).await?;
        let buy_is_phantom = self.is_order_phantom(&buy_id).await?;

        if sell_is_phantom || buy_is_phantom {
            warn!("👽 Phantom order detected: sell: `{}`, buy: `{}`.", &sell_order.order_id, &buy_order.order_id);
        }

        Ok(sell_is_phantom || buy_is_phantom)
    }

    async fn is_order_phantom(&self, order_id: &Bits256) -> Result<bool> {
        let order = self.orderbook.order_by_id(order_id).await?;
        Ok(order.value.is_none())
    }

    async fn match_pairs(&self, match_pairs: Vec<(String, String)>) -> Result<()> {
        if match_pairs.is_empty() {
            return Ok(());
        }

        match self.orderbook.match_in_pairs(
            match_pairs
                .iter()
                .map(|(s, b)| {
                    (
                        Bits256::from_hex_str(s).unwrap(),
                        Bits256::from_hex_str(b).unwrap(),
                    )
                })
                .collect(),
        ).await {
            Ok(_) => {
                info!("✅ Matched these pairs (sell, buy): => `{:#?}`!\n", &match_pairs);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                error!("matching error `{}`", e);
                error!("Tried to match these pairs, but failed: (sell, buy) => `{:#?}`.", &match_pairs);
                tokio::time::sleep(Duration::from_millis(500)).await;
                return Err(e.into());
            }
        }
        Ok(())
    }
}

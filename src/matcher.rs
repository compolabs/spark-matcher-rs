use ::log::{debug, error, info};
use anyhow::{Context, Result};
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use orderbook::{constants::RPC, orderbook_utils::Orderbook};
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;

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
    async fn do_match(&mut self) -> Result<()> {
        let mut sell_orders = fetch_orders_from_indexer(OrderType::Sell)
            .await
            .context("Failed to fetch sell orders")?;
        let mut buy_orders = fetch_orders_from_indexer(OrderType::Buy)
            .await
            .context("Failed to fetch buy orders")?;

        //TODO implement the matching algorithm
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

    async fn match_orders(&self, sell_order: &SpotOrder, buy_order: &SpotOrder) -> Result<()> {
        match self
            .orderbook
            .match_orders(
                &Bits256::from_hex_str(&sell_order.id).unwrap(),
                &Bits256::from_hex_str(&buy_order.id).unwrap(),
            )
            .await
        {
            Ok(_) => {
                info!(
                    "✅ Matched these pairs (sell, buy): => `{:#?}, {:#?}`!\n",
                    &sell_order.id, &buy_order.id
                );
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
            Err(e) => {
                error!("matching error `{}`", e);
                error!(
                    "Tried to match these pairs, but failed: (sell, buy) => `{:#?}, {:#?}`.",
                    &sell_order.id, &buy_order.id
                );
                tokio::time::sleep(Duration::from_millis(5000)).await;
                return Err(e.into());
            }
        }
        Ok(())
    }

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
        info!("Matcher launched, running...");
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
}

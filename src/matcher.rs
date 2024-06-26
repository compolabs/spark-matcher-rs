use ::log::{debug, error, info};
use anyhow::{Context, Result};
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::{Bits256, ContractId},
};
use reqwest::Client;
use spark_market_sdk::MarketContract;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::common::*;

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    market: MarketContract,
    initialized: bool,
    status: Status,
    ignore_list: Vec<String>,
    client: Client,
}

impl SparkMatcher {
    pub async fn new(client: Client) -> Result<Self> {
        let provider = Provider::connect("testnet.fuel.network").await?;
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );

        debug!("Setup SparkMatcher correctly.");

        Ok(Self {
            market: MarketContract::new(ContractId::from_str(&contract_id).unwrap(), wallet).await,
            initialized: true,
            status: Status::Chill,
            ignore_list: vec![],
            client,
        })
    }

    pub async fn init(client: Client) -> Result<Arc<Mutex<Self>>> {
        let result = Arc::new(Mutex::new(SparkMatcher::new(client).await?));
        Ok(result)
    }

    pub async fn run(&mut self) {
        info!("Matcher launched, running...\n");
        self.process_next().await;
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

            match self.do_match().await {
                Ok(_) => (),
                Err(e) => {
                    error!("An error occurred while matching: `{}`", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }

            self.ignore_list.clear();
            self.status = Status::Chill;
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }

    async fn do_match(&mut self) -> Result<()> {
        let (sell_orders_result, buy_orders_result) = tokio::join!(
            fetch_orders_from_indexer(crate::common::OrderType::Sell, &self.client),
            fetch_orders_from_indexer(crate::common::OrderType::Buy, &self.client)
        );

        let sell_orders = sell_orders_result.context("Failed to fetch sell orders")?;
        let buy_orders = buy_orders_result.context("Failed to fetch buy orders")?;

        if let Err(_e) = self.match_many(sell_orders, buy_orders).await {
            error!("Failed to match : {}\n", _e);
        }
        Ok(())
    }

    async fn match_many(
        &self,
        mut sell_orders: Vec<SpotOrder>,
        mut buy_orders: Vec<SpotOrder>,
    ) -> Result<()> {
        if sell_orders.is_empty() || buy_orders.is_empty() {
            return Ok(());
        }

        let start = Instant::now();

        // filter_orders(&mut buy_orders, &mut sell_orders);

        // if sell_orders.is_empty() || buy_orders.is_empty() {
        //     return Ok(());
        // }
        let order_pairs = create_order_pairs(buy_orders, sell_orders);
        let order_pairs_len = order_pairs.len();

        match self.market.match_order_many(order_pairs).await {
            Ok(res) => {
                info!(
                    "✅✅✅ Matched {} orders\nhttps://app.fuel.network/tx/0x{}/simple\n",
                    order_pairs_len,
                    res.tx_id.unwrap().to_string(),
                );
            }
            Err(e) => {
                error!("matching error `{}`\n", e);
                error!(
                    "Tried to match {} orders, but failed: (sell, buy).",
                    order_pairs_len,
                );
                return Err(e.into());
            }
        }
        let duration = start.elapsed();
        info!("SparkMatcher::match_pairs executed in {:?}", duration);
        Ok(())
    }
}

fn filter_orders(buy_orders: &mut Vec<SpotOrder>, sell_orders: &mut Vec<SpotOrder>) {
    let mut i = 0;
    let mut j = 0;

    while i < buy_orders.len() && j < sell_orders.len() {
        if buy_orders[i].price >= sell_orders[j].price {
            // Match found, move to the next order in both lists
            i += 1;
            j += 1;
        } else {
            if buy_orders[i].price < sell_orders[j].price {
                // Buy order cannot match, remove it
                buy_orders.remove(i);
            } else {
                // Sell order cannot match, remove it
                sell_orders.remove(j);
            }
        }
    }

    // Remove remaining unmatched sell orders
    while j < sell_orders.len() {
        sell_orders.remove(j);
    }

    // Remove remaining unmatched buy orders
    while i < buy_orders.len() {
        buy_orders.remove(i);
    }
}

fn create_order_pairs(buy_orders: Vec<SpotOrder>, sell_orders: Vec<SpotOrder>) -> Vec<Bits256> {
    let mut pairs = Vec::new();

    let mut buy_iter = buy_orders.into_iter();
    let mut sell_iter = sell_orders.into_iter();

    while let (Some(buy_order), Some(sell_order)) = (buy_iter.next(), sell_iter.next()) {
        // if buy_order.price >= sell_order.price {
        pairs.push(Bits256::from_hex_str(buy_order.id.as_str()).unwrap());
        pairs.push(Bits256::from_hex_str(sell_order.id.as_str()).unwrap());
        // }
    }

    pairs
}

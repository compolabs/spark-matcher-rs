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

        let mut matches: Vec<String> = Vec::new();

        let mut buy_index = 0;
        let mut sell_index = 0;

        while buy_index < buy_orders.len() && sell_index < sell_orders.len() {
            let buy_order = &mut buy_orders[buy_index];
            let sell_order = &mut sell_orders[sell_index];

            if buy_order.price.parse::<u128>().unwrap() >= sell_order.price.parse::<u128>().unwrap()
            {
                let buy_order_amount = buy_order.amount.parse::<u128>().unwrap();
                let sell_order_amount = sell_order.amount.parse::<u128>().unwrap();
                let match_amount = std::cmp::min(buy_order_amount, sell_order_amount);

                if !matches.contains(&buy_order.id) {
                    matches.push(buy_order.id.clone());
                }
                if !matches.contains(&sell_order.id) {
                    matches.push(sell_order.id.clone());
                }
                buy_order.amount = (buy_order_amount - match_amount).to_string();
                sell_order.amount = (sell_order_amount - match_amount).to_string();

                if buy_order.amount == "0" {
                    buy_index += 1;
                }

                if sell_order.amount == "0" {
                    sell_index += 1;
                }
            } else {
                sell_index += 1;
            }
        }

        let matches_len = matches.len();
        if matches_len == 0 {
            return Ok(());
        }
        let matches = matches
            .into_iter()
            .map(|id| Bits256::from_hex_str(&id).unwrap())
            .collect();
        match self.market.match_order_many(matches).await {
            Ok(res) => {
                info!(
                    "✅✅✅ Matched {} orders\nhttps://app.fuel.network/tx/0x{}/simple\n",
                    matches_len,
                    res.tx_id.unwrap().to_string(),
                );
            }
            Err(e) => {
                error!("matching error `{}`\n", e);
                error!(
                    "Tried to match {} orders, but failed: (sell, buy).",
                    matches_len,
                );
                return Err(e.into());
            }
        }
        let duration = start.elapsed();
        info!("SparkMatcher::match_pairs executed in {:?}", duration);
        Ok(())
    }
}

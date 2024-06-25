use ::log::{debug, error, info};
use anyhow::{Context, Result};
use fuels::{
    crypto::SecretKey,
    macros::abigen,
    prelude::{Provider, WalletUnlocked},
    types::{bech32::Bech32ContractId, Bits256, ContractId},
};
use reqwest::Client;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::common::*;

abigen!(Contract(
    name = "Market",
    abi = "market-contract/out/release/market-contract-abi.json"
));

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    market: Market<WalletUnlocked>,
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
            market: Market::new(
                Bech32ContractId::from(ContractId::from_str(&contract_id).unwrap()),
                wallet,
            ),
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
        info!("Matcher launched, running...");
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

            // let start = Instant::now();
            match self.do_match().await {
                Ok(_) => (),
                Err(e) => {
                    error!("An error occurred while matching: `{}`", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            // let duration = start.elapsed();
            // info!("SparkMatcher::process_next executed in {:?}", duration);

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

        let mut sell_orders = sell_orders_result.context("Failed to fetch sell orders")?;
        let mut buy_orders = buy_orders_result.context("Failed to fetch buy orders")?;

        let match_pairs = self
            .find_matching_pairs(&mut sell_orders, &mut buy_orders)
            .await?;

        if !match_pairs.is_empty() {
            if let Err(_e) = self.match_pairs(match_pairs.clone()).await {
                error!("Failed to match single pair: {}", _e);
            }
        }
        Ok(())
    }

    async fn find_matching_pairs(
        &mut self,
        sell_orders: &mut Vec<SpotOrder>,
        buy_orders: &mut Vec<SpotOrder>,
    ) -> Result<Vec<(String, String)>> {
        let mut match_pairs: Vec<(String, String)> = vec![];
        let mut sell_index = 0;
        let mut buy_index = 0;

        while sell_index < sell_orders.len() && buy_index < buy_orders.len() {
            let sell_order = sell_orders.get_mut(sell_index).unwrap();
            let buy_order = buy_orders.get_mut(buy_index).unwrap();

            let (sell_size, sell_price, buy_size, buy_price) = (
                sell_order
                    .amount
                    .parse::<i128>()
                    .context("Invalid sell order size")?,
                sell_order
                    .price
                    .parse::<i128>()
                    .context("Invalid sell order price")?,
                buy_order
                    .amount
                    .parse::<i128>()
                    .context("Invalid buy order size")?,
                buy_order
                    .price
                    .parse::<i128>()
                    .context("Invalid buy order price")?,
            );

            if sell_size == 0 {
                sell_index += 1;
                continue;
            }
            if buy_size == 0 {
                buy_index += 1;
                continue;
            }

            if self.match_conditions_met(
                sell_price, buy_price, sell_size, buy_size, sell_order, buy_order,
            ) {
                // let is_phantom_order_start = Instant::now();
                // if self.is_phantom_order(sell_order, buy_order).await? {
                //     sell_order.amount = "0".to_string();
                //     buy_order.amount = "0".to_string();
                //     // let is_phantom_order_duration = is_phantom_order_start.elapsed();
                //     // info!("SparkMatcher::is_phantom_order executed in {:?}", is_phantom_order_duration);
                //     continue;
                // }
                // let is_phantom_order_duration = is_phantom_order_start.elapsed();
                // info!("SparkMatcher::is_phantom_order executed in {:?}", is_phantom_order_duration);

                let amount = sell_size.abs().min(buy_size);
                sell_order.amount = (sell_size + amount).to_string();
                buy_order.amount = (buy_size - amount).to_string();

                match_pairs.push((sell_order.id.clone(), buy_order.id.clone()));

                if sell_order.amount == "0" {
                    sell_index += 1;
                }

                if buy_order.amount == "0" {
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
            && sell_order.asset == buy_order.asset
            && sell_size > 0
            && buy_size > 0
            && sell_price <= buy_price
    }

    // async fn is_phantom_order(
    //     &mut self,
    //     sell_order: &SpotOrder,
    //     buy_order: &SpotOrder,
    // ) -> Result<bool> {
    //     // let start = Instant::now();
    //     let sell_id = Bits256::from_hex_str(&sell_order.id)?;
    //     let buy_id = Bits256::from_hex_str(&buy_order.id)?;

    //     let (sell_is_phantom, buy_is_phantom) = try_join!(
    //         self.is_order_phantom(&sell_id),
    //         self.is_order_phantom(&buy_id)
    //     )?;

    //     if sell_is_phantom {
    //         warn!("👽 Phantom order detected: sell: `{}`.", &sell_order.id);
    //         self.ignore_list.push(sell_order.id.clone())
    //     }

    //     if buy_is_phantom {
    //         warn!("👽 Phantom order detected: buy: `{}`.", &buy_order.id);
    //         self.ignore_list.push(buy_order.id.clone())
    //     }

    //     // let duration = start.elapsed();
    //     // info!("SparkMatcher::is_phantom_order executed in {:?}", duration);

    //     Ok(sell_is_phantom || buy_is_phantom)
    // }

    // async fn is_order_phantom(&self, id: &Bits256) -> Result<bool> {
    //     // let start = Instant::now();
    //     let order = self.market.methods().order(id.clone()).simulate().await?;
    //     // let duration = start.elapsed();
    //     // info!("SparkMatcher::is_order_phantom executed in {:?}", duration);
    //     Ok(order.value.is_none())
    // }

    async fn match_pairs(&self, match_pairs: Vec<(String, String)>) -> Result<()> {
        if match_pairs.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        let pairs: Vec<_> = match_pairs
            .clone()
            .into_iter()
            .flat_map(|(s1, s2)| {
                vec![
                    Bits256::from_hex_str(&s1).unwrap(),
                    Bits256::from_hex_str(&s2).unwrap(),
                ]
            })
            .collect();

        match self.market.methods().match_order_many(pairs).call().await {
            Ok(res) => {
                info!(
                    "✅✅✅ Matched {} pairs\nhttps://app.fuel.network/tx/0x{}/simple\n",
                    match_pairs.len(),
                    // gas_price_res.gas_price as f64 * res.gas_used as f64 / 1e9f64, //todo
                    res.tx_id.unwrap().to_string()
                );
            }
            Err(e) => {
                error!("matching error `{}`", e);
                error!(
                    "Tried to match {} pairs, but failed: (sell, buy).",
                    match_pairs.len(),
                );
                return Err(e.into());
            }
        }
        let duration = start.elapsed();
        info!("SparkMatcher::match_pairs executed in {:?}", duration);
        Ok(())
    }

    // async fn match_single_pair(&self, pair: (String, String)) -> Result<()> {
    //     let start = Instant::now();
    //     let (sell, buy) = pair;
    //     let res = self
    //         .market
    //         .methods()
    //         .match_order_pair(
    //             Bits256::from_hex_str(&sell).unwrap(),
    //             Bits256::from_hex_str(&buy).unwrap(),
    //         )
    //         .call()
    //         .await?;

    //     info!(
    //         "✅ Matched single pair (sell, buy): => `({}, {})`\ntx id: {}\n",
    //         sell,
    //         buy,
    //         // res.gas_used as f64 * gas_price_res.gas_price as f64 / 1e9f64,//todo
    //         res.tx_id.unwrap().to_string()
    //     );
    //     let duration = start.elapsed();
    //     info!("SparkMatcher::match_single_pair executed in {:?}", duration);
    //     Ok(())
    // }
}

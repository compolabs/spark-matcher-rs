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

    // TODO: add more debug logging to the new one-to-many algorithm
    // TODO: the new one-to-many matches some of the matching orders, but not all of them:
    //       investigate for bugs?
    async fn do_match(&mut self) -> Result<()> {
        let (mut sell_orders, mut buy_orders) = (
            fetch_orders_from_indexer(OrderType::Sell).await?,
            fetch_orders_from_indexer(OrderType::Buy).await?,
        );
        debug!(
            "Sell orders for this match: `{:#?}`\n\nBuy orders for this match: `{:#?}`\n\n",
            &sell_orders, &buy_orders
        );

        let mut sell_index = 0_usize;
        for the_buy in &mut buy_orders {
            let buy_id = Bits256::from_hex_str(&the_buy.order_id)?;
            let mut sells: Vec<String> = vec![];
            let (buy_size, buy_price) = (
                the_buy.base_size.parse::<i128>()?,
                the_buy.base_price.parse::<i128>()?,
            );
            if buy_size == 0 {
                continue;
            }

            let mut transaction_amount: i128 = 0;
            let mut bail = false;
            let sell_start = sell_index;
            while sell_index < sell_orders.len() && buy_size > 0 {
                let current_sell = sell_orders.get_mut(sell_index).unwrap();
                let (sell_size, sell_price) = (
                    current_sell.base_size.parse::<i128>()?,
                    current_sell.base_price.parse::<i128>()?,
                );
                if sell_price > buy_price {
                    bail = true;
                    break;
                }
                if sell_size >= 0 || the_buy.base_token != current_sell.base_token {
                    sell_index += 1;
                    continue;
                }

                let sell_id = Bits256::from_hex_str(&current_sell.order_id)?;
                if self.orderbook.order_by_id(&sell_id).await?.value.is_none() {
                    warn!("👽 Phantom order sell: `{}`.", &current_sell.order_id);

                    current_sell.base_size = 0.to_string();
                    sell_index += 1;
                    continue;
                }
                if self.orderbook.order_by_id(&buy_id).await?.value.is_none() {
                    warn!("👽 Phantom order buy: `{}`.", &the_buy.order_id);

                    the_buy.base_size = 0.to_string();
                    break;
                }

                if transaction_amount + sell_size.abs() > buy_size {
                    break;
                } else {
                    transaction_amount += sell_size.abs();
                    sells.push(current_sell.order_id.clone());
                    sell_index += 1;
                }
            }

            if sells.is_empty() {
                continue;
            }
            let sell_end = sell_start + sells.len();

            match self
                .orderbook
                .match_orders_many(
                    sells
                        .iter()
                        .map(|id| Bits256::from_hex_str(id).unwrap())
                        .collect(),
                    vec![buy_id],
                )
                .await
            {
                Ok(_) => {
                    info!(
                        "✅ SUCCESS: buy `{}` matched with sells {:#?}\n",
                        &the_buy.order_id, &sells
                    );

                    for si in sell_start..sell_end {
                        sell_orders.get_mut(si).unwrap().base_size = 0.to_string();
                    }
                    the_buy.base_size = (buy_size - transaction_amount).to_string();

                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    error!("Error while matching: `{}`", e);

                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }

            if bail {
                return Ok(());
            }
        }

        Ok(())
    }
}

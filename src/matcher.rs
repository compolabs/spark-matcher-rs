use ::log::{debug, error, info, warn};
use anyhow::Result;
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use tokio::sync::Mutex;

use orderbook::{constants::RPC, orderbook_utils::Orderbook};

use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

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
    fails: HashMap<String, i64>,
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
            fails: HashMap::new(),
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
        let (mut sell_orders, mut buy_orders) = (
            fetch_orders_from_indexer(OrderType::Sell).await?,
            fetch_orders_from_indexer(OrderType::Buy).await?,
        );
        debug!("Sell orders: `{:#?}`", &sell_orders);
        debug!("Buy orders: `{:#?}`", &buy_orders);
        for sell_order in &mut sell_orders {
            let (sell_size, sell_price) = (
                sell_order.base_size.parse::<i128>()?,
                sell_order.base_price.parse::<i128>()?,
            );
            if sell_size == 0 {
                continue;
            }
            if *self.fails.get(&sell_order.order_id).unwrap_or(&0) > 5 {
                continue;
            }
            for buy_order in &mut buy_orders {
                let (buy_size, buy_price) = (
                    buy_order.base_size.parse::<i128>()?,
                    buy_order.base_price.parse::<i128>()?,
                );
                if buy_size == 0 {
                    continue;
                }
                if *self.fails.get(&buy_order.order_id).unwrap_or(&0) > 5 {
                    continue;
                }
                let sell_id = Bits256::from_hex_str(&sell_order.order_id)?;
                let buy_id = Bits256::from_hex_str(&buy_order.order_id)?;

                debug!("==== prices before matching ====\nSell price: `{}`;\n Sell size: `{}`\nBuy price: `{}`;\nBuy size: `{}`;\n ========= end =========", sell_price, sell_size, buy_price, buy_size);

                let price_cond = sell_price <= buy_price;
                let sell_size_cond = sell_size < 0;
                let buy_size_cond = buy_size > 0;
                let token_cond = sell_order.base_token == buy_order.base_token;

                debug!("===== Conditions: =====\nsell_price <= buy_price: `{}`;\nsell_size < 0: `{}`;\nbuy_size > 0: `{}`;\nsell_order.base_token == buy_order.base_token: `{}`\nsell token: `{}`;\nbuy_token: `{}`\n", price_cond, sell_size_cond, buy_size_cond, token_cond, sell_order.base_token, buy_order.base_token);

                if price_cond && sell_size_cond && buy_size_cond && token_cond {
                    if self.orderbook.order_by_id(&sell_id).await?.value.is_none() {
                        warn!("👽 Phantom order sell: `{}`.", &sell_order.order_id);
                        let sell_fail = self.fails.entry(sell_order.order_id.clone()).or_insert(0);
                        *sell_fail += 1;

                        continue;
                    }
                    if self.orderbook.order_by_id(&buy_id).await?.value.is_none() {
                        warn!("👽 Phantom order buy: `{}`.", &buy_order.order_id);
                        let buy_fail = self.fails.entry(buy_order.order_id.clone()).or_insert(0);
                        *buy_fail += 1;

                        continue;
                    }

                    match self.orderbook.match_orders(&sell_id, &buy_id).await {
                        Ok(_) => {
                            info!(
                                "✅ Orders matched: sell => `{}`, buy => `{}`!\n",
                                &sell_order.order_id, &buy_order.order_id
                            );
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            // let amount = if (sell_size.abs()) > buy_size {
                            //     buy_size
                            // } else {
                            //     sell_size.abs()
                            // };

                            // println!("sell size before: `{}`", &sell_order.base_size);
                            // println!("buy size before: `{}`", &buy_order.base_size);
                            // sell_order.base_size = (sell_size + amount).to_string();
                            // buy_order.base_size = (buy_size - amount).to_string();
                            // println!("sell size after: `{}`", &sell_order.base_size);
                            // println!("buy size after: `{}`", &buy_order.base_size);
                        }
                        Err(e) => {
                            error!("matching error `{}`", e);
                            let sell_fail =
                                self.fails.entry(sell_order.order_id.clone()).or_insert(0);
                            *sell_fail += 1;

                            let buy_fail =
                                self.fails.entry(buy_order.order_id.clone()).or_insert(0);
                            *buy_fail += 1;

                            tokio::time::sleep(Duration::from_millis(1000)).await;
                        }
                    };
                }
            }
        }

        Ok(())
    }
}

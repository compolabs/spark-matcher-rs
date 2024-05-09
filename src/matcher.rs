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
        let (mut sell_orders, mut buy_orders) = (
            fetch_orders_from_indexer(OrderType::Sell).await?,
            fetch_orders_from_indexer(OrderType::Buy).await?,
        );
        debug!(
            "Sell orders for this match: `{:#?}`\n\nBuy orders for this match: `{:#?}`\n\n",
            &sell_orders, &buy_orders
        );

        let mut sell_index = 0 as usize;
        let mut buy_index = 0 as usize;
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

                    sell_order.base_size = 0.to_string();
                    sell_index += 1;
                    continue;
                }
                if self.orderbook.order_by_id(&buy_id).await?.value.is_none() {
                    warn!("👽 Phantom order buy: `{}`.", &buy_order.order_id);

                    buy_order.base_size = 0.to_string();
                    buy_index += 1;
                    continue;
                }

                match self
                    .orderbook
                    .match_orders_many(vec![sell_id], vec![buy_id])
                    .await
                {
                    Ok(_) => {
                        info!(
                            "✅ Orders matched: sell => `{}`, buy => `{}`!\n",
                            &sell_order.order_id, &buy_order.order_id
                        );
                        let amount = if (sell_size.abs()) > buy_size {
                            buy_size
                        } else {
                            sell_size.abs()
                        };

                        sell_order.base_size = (sell_size + amount).to_string();
                        buy_order.base_size = (buy_size - amount).to_string();

                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        error!("matching error `{}`", e);

                        sell_index += 1;
                        buy_index += 1;

                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                };
            } else if !price_cond {
                break;
            }
        }

        Ok(())
    }
}

use crate::common::*;
use ::log::{debug, error, info, warn};
use anyhow::{anyhow, Result};
use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::Bits256,
};
use orderbook::{constants::RPC, orderbook_utils::Orderbook};
use std::{collections::HashSet, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;

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
        let mut max_batch_size = ev("FETCH_ORDER_LIMIT")
            .unwrap_or("1000".to_string())
            .parse::<usize>()?
            / 10;
        if max_batch_size < 1 {
            max_batch_size = 1;
        }
        debug!("Max batch size for this match is: `{}`.", max_batch_size);

        let (sell_orders, buy_orders) = tokio::join!(
            fetch_orders_from_indexer(OrderType::Sell),
            fetch_orders_from_indexer(OrderType::Buy)
        );

        let mut sell_orders = sell_orders?;
        let mut buy_orders = buy_orders?;

        debug!(
            "Sell orders for this match: `{:#?}`\n\nBuy orders for this match: `{:#?}`\n\n",
            &sell_orders, &buy_orders
        );

        let mut sell_index = 0;
        let mut buy_index = 0;
        let mut sell_batch: HashSet<String> = HashSet::new();
        let mut buy_batch: HashSet<String> = HashSet::new();

        while sell_index < sell_orders.len() && buy_index < buy_orders.len() {
            if let Some(sell_order) = sell_orders.get_mut(sell_index) {
                if let Some(buy_order) = buy_orders.get_mut(buy_index) {
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
                        } else if self.orderbook.order_by_id(&buy_id).await?.value.is_none() {
                            warn!("👽 Phantom order buy: `{}`.", &buy_order.order_id);
                            buy_order.base_size = 0.to_string();
                        } else {
                            let amount = sell_size.abs().min(buy_size);
                            sell_order.base_size = (sell_size + amount).to_string();
                            buy_order.base_size = (buy_size - amount).to_string();

                            // Add to batches
                            sell_batch.insert(sell_order.order_id.clone());
                            buy_batch.insert(buy_order.order_id.clone());
                            debug!(
                            "Found matching orders, adding to batches. sell => `{}`, buy => `{}`!\n",
                            &sell_order.order_id, &buy_order.order_id
                        );
                        }
                    } else if !price_cond {
                        debug!(
                            "Bail condition, sell batch: `{:#?}`, buy batch: `{:#?}`",
                            &sell_batch, &buy_batch
                        );
                        if !sell_batch.is_empty() && !buy_batch.is_empty() {
                            self.match_orders(&sell_batch, &buy_batch).await?;
                            sell_batch.clear();
                            buy_batch.clear();
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        break;
                    }
                }
            }
            sell_index += 1;
            buy_index += 1;
        }
        Ok(())
    }

    async fn match_orders(
        &mut self,
        sell_batch: &HashSet<String>,
        buy_batch: &HashSet<String>,
    ) -> Result<()> {
        match self
            .orderbook
            .match_orders_many(
                sell_batch
                    .iter()
                    .map(|s| Bits256::from_hex_str(s).unwrap())
                    .collect(),
                buy_batch
                    .iter()
                    .map(|b| Bits256::from_hex_str(b).unwrap())
                    .collect(),
            )
            .await
        {
            Ok(_) => {
                info!(
                    "✅ Matched two batches: sells => `{:#?}`, buys => `{:#?}`!\n",
                    &sell_batch, &buy_batch
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                error!("matching error `{}`", e);
                error!(
                    "Tried to match these batches, but failed: sells => `{:#?}`, buys: `{:#?}`.",
                    sell_batch, buy_batch
                );
                tokio::time::sleep(Duration::from_millis(500)).await;
                return Err(e.into());
            }
        };
        Ok(())
    }
}

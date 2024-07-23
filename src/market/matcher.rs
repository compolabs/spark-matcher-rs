use chrono::Utc;
use fuels::programs::call_response::FuelCallResponse;
use fuels::types::{Bits256, Bytes32};
use fuels::{
    accounts::provider::Provider,
    accounts::wallet::WalletUnlocked,
    types::ContractId,
    crypto::SecretKey,
};
use log::{info, error};
use sqlx::types::BigDecimal;
use tokio::time::Instant;
use std::str::FromStr;
use std::sync::Arc;
use crate::config::ev;
use crate::management::manager::OrderManager;
use crate::model::{SpotOrder, OrderType};
use crate::error::Error;
use spark_market_sdk::MarketContract;
use sqlx::PgPool;
use serde_json::json;

pub struct SparkMatcher {
    pub order_manager: Arc<OrderManager>,
    pub market: MarketContract,
    pub db_pool: PgPool,
}

impl SparkMatcher {
    pub async fn new(order_manager: Arc<OrderManager>) -> Result<Self, Error> {
        let provider = Provider::connect("testnet.fuel.network").await?;
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let secret_key = SecretKey::from_str(&private_key).unwrap();
        let wallet = WalletUnlocked::new_from_private_key(secret_key, Some(provider.clone()));
        let market = MarketContract::new(ContractId::from_str(&contract_id)?, wallet).await;

        let database_url = ev("DATABASE_URL")?;
        let db_pool = PgPool::connect(&database_url).await.unwrap();

        Ok(Self {
            order_manager,
            market,
            db_pool,
        })
    }

    pub async fn run(&self) -> Result<(), Error> {
        loop {
            if let Err(e) = self.match_orders().await {
                error!("Error during matching orders: {:?}", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    pub async fn match_orders(&self) -> Result<(), Error> {
        info!("-----Trying to match orders");
        let mut buy_orders = self.order_manager.buy_orders.write().await;
        let mut sell_orders = self.order_manager.sell_orders.write().await;
        info!("sell: {:?}", sell_orders.len());
        info!("buy: {:?}", buy_orders.len());

        let start = Instant::now();
        let mut matches: Vec<(String, String, u128)> = Vec::new();
        let mut total_amount: u128 = 0;

        for (&buy_price, buy_list) in buy_orders.iter_mut() {
            for (&sell_price, sell_list) in sell_orders.range_mut(..=buy_price) {
                let mut buy_index = 0;
                let mut sell_index = 0;

                while buy_index < buy_list.len() && sell_index < sell_list.len() {
                    let buy_order = &mut buy_list[buy_index];
                    let sell_order = &mut sell_list[sell_index];

                    if buy_order.price >= sell_order.price {
                        let buy_order_amount = buy_order.amount;
                        let sell_order_amount = sell_order.amount;
                        let match_amount = std::cmp::min(buy_order_amount, sell_order_amount);

                        matches.push((buy_order.id.clone(), sell_order.id.clone(), match_amount));
                        total_amount += match_amount;

                        buy_order.amount -= match_amount;
                        sell_order.amount -= match_amount;

                        if buy_order.amount == 0 {
                            buy_index += 1;
                        }

                        if sell_order.amount == 0 {
                            sell_index += 1;
                        }
                    } else {
                        sell_index += 1;
                    }
                }
            }
        }

        buy_orders.retain(|_, orders| {
            orders.retain(|order| order.amount > 0);
            !orders.is_empty()
        });

        sell_orders.retain(|_, orders| {
            orders.retain(|order| order.amount > 0);
            !orders.is_empty()
        });

        let matches_len = matches.len();
        if matches_len == 0 {
            return Ok(());
        }

        let matches: Vec<Bits256> = matches
            .into_iter()
            .flat_map(|(buy_id, sell_id, _)| vec![Bits256::from_hex_str(&buy_id).unwrap(), Bits256::from_hex_str(&sell_id).unwrap()])
            .collect();
        let res = self.market.match_order_many(matches).await;

        match res {
            Ok(r) =>{
                info!(
                    "✅✅✅ Matched {} orders\nhttps://app.fuel.network/tx/0x{}/simple\n",
                    matches_len,
                    r.tx_id.unwrap().to_string(),
                );
                self.log_transaction(total_amount, matches_len, &r).await?;
            }
            Err(e) => {
                error!("matching error `{}`\n", e);
                return Err(Error::MatchOrdersError(e.to_string()));
            }
        };

        let duration = start.elapsed();
        info!("SparkMatcher::match_orders executed in {:?}", duration);

        Ok(())
    }

     async fn log_transaction(&self, total_amount: u128, matches_len: usize, res: &FuelCallResponse<()>) -> Result<(), Error> {
        let amount = total_amount.to_string(); 
        let order_received_time = Utc::now().to_rfc3339(); 
        let match_time = order_received_time.clone(); // Это время, когда происходит матчинг
        let details = json!({
            "matches_len": matches_len,
            "tx_id": res.tx_id.unwrap().to_string()
        });

        sqlx::query!(
            r#"
            INSERT INTO transactions (amount, order_received_time, match_time, details)
            VALUES ($1, $2, $3, $4)
            "#,
            amount,
            order_received_time,
            match_time,
            details
        )
        .execute(&self.db_pool)
        .await.unwrap();

        Ok(())
    }
}

impl OrderManager {
    pub async fn get_all_orders(&self) -> (Vec<SpotOrder>, Vec<SpotOrder>) {
        let buy_orders = self.buy_orders.read().await;
        let sell_orders = self.sell_orders.read().await;

        let buy_list = buy_orders.values().flat_map(|v| v.clone()).collect();
        let sell_list = sell_orders.values().flat_map(|v| v.clone()).collect();

        (buy_list, sell_list)
    }
}

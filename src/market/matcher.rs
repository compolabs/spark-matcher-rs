use crate::config::ev;
use crate::error::Error;
use crate::logger::{log_transactions, TransactionLog};
use crate::management::manager::OrderManager;
use crate::model::SpotOrder;
use fuels::types::Bits256;
use fuels::{
    accounts::provider::Provider, accounts::wallet::WalletUnlocked,
    types::ContractId,
};
use log::{error, info};
use spark_market_sdk::MarketContract;
use sqlx::PgPool;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub struct SparkMatcher {
    pub order_manager: Arc<OrderManager>,
    pub market: MarketContract,
    pub log_sender: mpsc::UnboundedSender<TransactionLog>,
    pub last_receive_time: Arc<tokio::sync::Mutex<Instant>>,
}

impl SparkMatcher {
    pub async fn new(order_manager: Arc<OrderManager>) -> Result<Self, Error> {
        let provider = Provider::connect("testnet.fuel.network").await?;
        let mnemonic = ev("MNEMONIC")?;
        let contract_id = ev("CONTRACT_ID")?;
        let wallet =
            WalletUnlocked::new_from_mnemonic_phrase(&mnemonic, Some(provider.clone())).unwrap();
        let market = MarketContract::new(ContractId::from_str(&contract_id)?, wallet).await;

        let database_url = ev("DATABASE_URL")?;
        let db_pool = PgPool::connect(&database_url).await.unwrap();

        let (log_sender, log_receiver) = mpsc::unbounded_channel();
        tokio::spawn(log_transactions(log_receiver, db_pool));

        Ok(Self {
            order_manager,
            market,
            log_sender,
            last_receive_time: Arc::new(tokio::sync::Mutex::new(Instant::now())),
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
        let receive_time = {
            let mut last_receive_time = self.last_receive_time.lock().await;
            let duration = last_receive_time.elapsed();
            *last_receive_time = Instant::now();
            duration.as_millis() as i64
        };

        info!("-----Trying to match orders");

        let match_start = Instant::now();
        info!("Match start time: {:?}", match_start);

        let mut buy_queue = BinaryHeap::new();
        let mut sell_queue = BinaryHeap::new();

        {
            let buy_orders = self.order_manager.buy_orders.read().await;
            for (_, orders) in buy_orders.iter() {
                for order in orders {
                    buy_queue.push(order.clone());
                }
            }

            let sell_orders = self.order_manager.sell_orders.read().await;
            for (_, orders) in sell_orders.iter() {
                for order in orders {
                    sell_queue.push(Reverse(order.clone()));
                }
            }
        }

        let mut matches: Vec<(String, String, u128)> = Vec::new();
        let mut total_amount: u128 = 0;

        while let (Some(mut buy_order), Some(Reverse(mut sell_order))) =
            (buy_queue.pop(), sell_queue.pop())
        {
            if buy_order.price >= sell_order.price {
                let match_amount = std::cmp::min(buy_order.amount, sell_order.amount);
                matches.push((buy_order.id.clone(), sell_order.id.clone(), match_amount));
                total_amount += match_amount;

                buy_order.amount -= match_amount;
                sell_order.amount -= match_amount;

                if buy_order.amount > 0 {
                    buy_queue.push(buy_order);
                }

                if sell_order.amount > 0 {
                    sell_queue.push(Reverse(sell_order));
                }
            } else {
                sell_queue.push(Reverse(sell_order));
            }
        }

        let match_duration = match_start.elapsed().as_millis() as i64;
        info!("Match duration calculated: {}", match_duration);

        let matches_len = matches.len();
        if matches_len == 0 {
            return Ok(());
        }

        let post_start = Instant::now();
        info!("Post start time: {:?}", post_start);

        let unique_order_ids: HashSet<String> = matches
            .iter()
            .flat_map(|(buy_id, sell_id, _)| vec![buy_id.clone(), sell_id.clone()])
            .collect();

        let unique_bits256_ids: Vec<Bits256> = unique_order_ids
            .iter()
            .map(|id| Bits256::from_hex_str(id).unwrap())
            .collect();
        /*
                    let formatted_log = self.format_order_info(
                        &matches.iter().map(|(buy_id, _, amount)| (buy_id.clone(), format!("Price: TBD"), *amount)).collect::<Vec<_>>(),
                        &matches.iter().map(|(_, sell_id, amount)| (sell_id.clone(), format!("Price: TBD"), *amount)).collect::<Vec<_>>()
                    );
                    info!("Matched Orders:\n{}", formatted_log);

                    let matches_human: Vec<String> = matches.clone()
                        .into_iter()
                        .flat_map(|(buy_id, sell_id, _)| vec![buy_id, sell_id])
                        .collect();

                    let matches: Vec<Bits256> = matches
                        .into_iter()
                        .flat_map(|(buy_id, sell_id, _)| vec![Bits256::from_hex_str(&buy_id).unwrap(), Bits256::from_hex_str(&sell_id).unwrap()])
                        .collect();
        */
        println!("=================================================");
        println!("=================================================");
        println!("matches {:?}", matches);
        println!("=================================================");
        println!("=================================================");
        let res = self.market.match_order_many(unique_bits256_ids).await;
        self.order_manager.clear_orders().await;
        /*
        let a = Bits256::from_hex_str("0x7e9927af85019fa02bc244477f72cb132a7a8b8ea6becf0e30f8a042de2f5397").unwrap();
        let b = Bits256::from_hex_str("0x48b64d43c40f3a70617475d345bfd709c233e43e3c78e44be42efb77a849f8bd").unwrap();

        println!("=======================================");
        println!("=======================================");
        println!("=======================================");
        let r = self.market.order(b).await.unwrap().value;
        println!("{:?}", r);
        let r1 = self.market.order_change_info(b).await.unwrap().value;
        println!("=======================================");
        //let r = self.market.order(b).await;
        println!("{:?}", r1);
        println!("=======================================");
        println!("=======================================");
        */

        match res {
            Ok(r) => {
                let post_duration = post_start.elapsed().as_millis() as i64;
                let log = TransactionLog {
                    total_amount,
                    matches_len,
                    tx_id: r.tx_id.unwrap().to_string(),
                    gas_used: r.gas_used,
                    match_time_ms: match_duration,
                    buy_orders: buy_queue.len(),
                    sell_orders: sell_queue.len(),
                    receive_time_ms: receive_time,
                    post_time_ms: post_duration,
                };
                info!("Logging transaction: {:?}", log);
                self.log_sender.send(log).unwrap();
                info!(
                    "✅✅✅ Matched {} orders\nhttps://app.fuel.network/tx/0x{}/simple\n",
                    matches_len,
                    r.tx_id.unwrap().to_string(),
                );
            }
            Err(e) => {
                error!("matching error `{}`\n", e);
                return Err(Error::MatchOrdersError(e.to_string()));
            }
        };

        Ok(())
    }

    fn format_order_info(
        &self,
        buy_orders: &[(String, String, u128)],
        sell_orders: &[(String, String, u128)],
    ) -> String {
        let mut logs = Vec::new();
        logs.push("🔵 Buy Orders:".to_string());
        for (id, price, amount) in buy_orders {
            logs.push(format!("ID: {}, Price: {}, Amount: {}", id, price, amount));
        }
        logs.push("🔴 Sell Orders:".to_string());
        for (id, price, amount) in sell_orders {
            logs.push(format!("ID: {}, Price: {}, Amount: {}", id, price, amount));
        }

        logs.join("\n")
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

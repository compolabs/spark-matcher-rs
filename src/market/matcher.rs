use crate::config::ev;
use crate::error::Error;
use crate::logger::{log_transactions, TransactionLog};
use crate::management::manager::OrderManager;
use crate::model::SpotOrder;
use fuels::types::Bits256;
use fuels::{accounts::provider::Provider, accounts::wallet::WalletUnlocked, types::ContractId};
use futures_util::future::join_all;
use log::{error, info};
use spark_market_sdk::MarketContract;
use sqlx::PgPool;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::Instant;

pub struct SparkMatcher {
    pub order_manager: Arc<OrderManager>,
    pub market: MarketContract,
    pub log_sender: mpsc::UnboundedSender<TransactionLog>,
    pub last_receive_time: Arc<tokio::sync::Mutex<Instant>>,
    pub additional_wallets: Vec<WalletUnlocked>,
    pub wallet: WalletUnlocked,
    pub provider: Provider,      // Keep the provider for reuse
    pub contract_id: ContractId, // Keep the contract_id for reuse
}

impl SparkMatcher {
    pub async fn new(order_manager: Arc<OrderManager>) -> Result<Self, Error> {
        let provider = Provider::connect("testnet.fuel.network").await?;
        let mnemonic = ev("MNEMONIC")?;
        let contract_id = ev("CONTRACT_ID")?;

        let wallet =
            WalletUnlocked::new_from_mnemonic_phrase(&mnemonic, Some(provider.clone())).unwrap();
        let contract_id = ContractId::from_str(&contract_id)?;
        let market = MarketContract::new(contract_id.clone(), wallet.clone()).await;

        let database_url = ev("DATABASE_URL")?;
        let db_pool = PgPool::connect(&database_url).await.unwrap();

        let (log_sender, log_receiver) = mpsc::unbounded_channel();
        tokio::spawn(log_transactions(log_receiver, db_pool));

        let additional_wallets: Vec<WalletUnlocked> = (0..3)
            .map(|i| {
                // m/44'/1179993420'/{n}'/0/0
                // https://github.com/FuelLabs/fuels-ts/blob/4e82ad42b84e520c3133907aabeea2aad1c1a199/packages/account/src/wallet-manager/vaults/mnemonic-vault.ts#L21
                let path = format!("m/44'/1179993420'/{}'/0/0", i);
                println!("path {}", path);
                let address = WalletUnlocked::new_from_mnemonic_phrase_with_path(
                    &mnemonic,
                    Some(provider.clone()),
                    &path,
                )
                .unwrap();
                println!("address: {}", address.address().to_string());
                WalletUnlocked::new_from_mnemonic_phrase_with_path(
                    &mnemonic,
                    Some(provider.clone()),
                    &path,
                )
                .unwrap()
            })
            .collect();

        // Log the public keys
        info!("Main wallet public key: {}", wallet.address().hash());
        for (i, additional_wallet) in additional_wallets.iter().enumerate() {
            info!(
                "Additional wallet {} public key: {}",
                i + 1,
                additional_wallet.address()
            );
        }

        Ok(Self {
            order_manager,
            market,
            log_sender,
            last_receive_time: Arc::new(tokio::sync::Mutex::new(Instant::now())),
            additional_wallets,
            wallet,
            provider,
            contract_id,
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
        info!("Main wallet public key: {}", self.wallet.address());
        for (i, additional_wallet) in self.additional_wallets.iter().enumerate() {
            info!(
                "Additional wallet {} public key: {}",
                i + 1,
                additional_wallet.address()
            );
        }

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

        println!("=================================================");
        println!("=================================================");
        // println!("matches {:?}", matches);
        println!("=================================================");
        println!("=================================================");

        // Split the matches and process in parallel with a maximum chunk size of 10
        let chunk_size = 10;
        let chunks: Vec<Vec<(String, String, u128)>> =
            matches.chunks(chunk_size).map(|c| c.to_vec()).collect();

        let semaphore = Arc::new(Semaphore::new(3)); // Limit to 3 concurrent tasks
        let mut tasks = vec![];

        for (i, chunk) in chunks.into_iter().enumerate() {
            let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();

            let market = if i == 0 {
                MarketContract::new(self.contract_id.clone(), self.wallet.clone()).await
            } else if i <= self.additional_wallets.len() {
                MarketContract::new(
                    self.contract_id.clone(),
                    self.additional_wallets[i - 1].clone(),
                )
                .await
            } else {
                // no more wallets, break out of for loop
                break;
            };

            let wallet_public_key = if i == 0 {
                self.wallet.address().to_string()
            } else if i <= self.additional_wallets.len() {
                self.additional_wallets[i - 1].address().to_string()
            } else {
                self.wallet.address().to_string()
            };

            // Convert match chunks to Bits256 IDs for market.match_order_many
            let chunk_bits256_ids: Vec<Bits256> = chunk
                .iter()
                .flat_map(|(buy_id, sell_id, _)| {
                    vec![
                        Bits256::from_hex_str(buy_id).unwrap(),
                        Bits256::from_hex_str(sell_id).unwrap(),
                    ]
                })
                .collect();

            println!("MATCHING: {:?}", chunk);

            let task = tokio::spawn(async move {
                let _permit = permit; // Hold permit until task is done
                match market.match_order_many(chunk_bits256_ids).await {
                    Ok(_) => {
                        println!("MATCHED: {:?}", chunk);
                        info!("✅✅✅ Matched orders\n");
                        Ok(())
                    }
                    Err(e) => {
                        error!(
                            "Error matching orders with wallet {}: {:?}",
                            wallet_public_key, e
                        );

                        Err(e)
                    }
                }
            });

            tasks.push(task);
        }

        let results = join_all(tasks).await;
        self.order_manager.clear_orders().await;

        for result in results {
            match result {
                Ok(Ok(())) => {
                    let post_duration = post_start.elapsed().as_millis() as i64;
                    let log = TransactionLog {
                        total_amount,
                        matches_len,
                        tx_id: String::new(), // Since tx_id is not available
                        gas_used: 0,          // Since gas_used is not available
                        match_time_ms: match_duration,
                        buy_orders: buy_queue.len(),
                        sell_orders: sell_queue.len(),
                        receive_time_ms: receive_time,
                        post_time_ms: post_duration,
                    };
                    info!("Logging transaction: {:?}", log);
                    self.log_sender.send(log).unwrap();
                    info!("✅✅✅ Matched {} orders\n", matches_len);
                }
                Ok(Err(e)) => {
                    error!("matching error `{}`\n", e);
                    return Err(Error::MatchOrdersError(e.to_string()));
                }
                Err(e) => {
                    error!("task join error `{}`\n", e);
                    return Err(Error::MatchOrdersError(e.to_string()));
                }
            }
        }

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

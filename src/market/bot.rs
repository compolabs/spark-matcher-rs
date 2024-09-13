use crate::config::ev;
use crate::error::Error;
use crate::management::manager::OrderManager;
use crate::model::SpotOrder;
use crate::random::random_generator::RandomGenerator;
use fuel_crypto::fuel_types::Address;
use fuels::accounts::ViewOnlyAccount;
use fuels::types::AssetId;
use fuels::{accounts::provider::Provider, accounts::wallet::WalletUnlocked, types::ContractId};
use log::error;
use spark_market_sdk::{OrderType, SparkMarketContract};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::Instant;
pub struct Bot {
    pub order_manager: Arc<OrderManager>,
    pub random: Arc<RandomGenerator>,
    pub market: SparkMarketContract,
    pub last_receive_time: Arc<tokio::sync::Mutex<Instant>>,
    btc_id: AssetId,
    usdc_id: AssetId,
    eth_id: AssetId,
    // multiasset_contract: MultiAssetContract,
    wallet: WalletUnlocked,
}

pub fn format_value_with_decimals(value: u64, decimals: u32) -> u64 {
    value * 10u64.pow(decimals)
}

pub fn format_to_readable_value(value: u64, decimals: u32) -> f64 {
    value as f64 / 10u64.pow(decimals) as f64
}

impl Bot {
    pub async fn new(
        order_manager: Arc<OrderManager>,
        random: Arc<RandomGenerator>,
    ) -> Result<Self, Error> {
        let provider = Provider::connect("testnet.fuel.network").await?;
        let mnemonic = ev("MNEMONIC")?;
        let contract_id = ev("BTC_USDC_MARKET_ID")?;
        let wallet =
            WalletUnlocked::new_from_mnemonic_phrase(&mnemonic, Some(provider.clone())).unwrap();
        println!("bot address = {:?}", wallet.address().hash());
        println!("market address = {:?}\n", contract_id);
        let market =
            SparkMarketContract::new(ContractId::from_str(&contract_id)?, wallet.clone()).await;

        let btc_id: String = ev("BTC_ID")?;
        let usdc_id: String = ev("USDC_ID")?;
        let eth_id: String = ev("ETH_ID")?;
        let btc_id = AssetId::from_str(&btc_id)?;
        let usdc_id = AssetId::from_str(&usdc_id)?;
        let eth_id = AssetId::from_str(&eth_id)?;

        // let multiasset_contract_id: String = ev("MULTIASSET_CONTRACT_ID")?;
        // let multiasset_contract_id = ContractId::from_str(&multiasset_contract_id)?.into();
        // let multiasset_contract = MultiAssetContract::new(&multiasset_contract_id, wallet.clone());

        Ok(Self {
            order_manager,
            random,
            market,
            last_receive_time: Arc::new(tokio::sync::Mutex::new(Instant::now())),
            btc_id,
            usdc_id,
            eth_id,
            // multiasset_contract,
            wallet,
        })
    }

    pub async fn run(&self) -> Result<(), Error> {
        loop {
            if let Err(e) = self.do_task().await {
                error!("Error during the task: {:?}", e);
            }
            // tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    }
    pub async fn do_task(&self) -> Result<(), Error> {
        let _ = self.last_receive_time.lock().await;

        let account_balances = self.wallet.get_balances().await.unwrap();
        let eth_account_balance = account_balances
            .get(&self.eth_id.to_string())
            .unwrap_or(&0)
            .to_owned();
        let usdc_account_balance = account_balances
            .get(&self.usdc_id.to_string())
            .unwrap_or(&0)
            .to_owned();
        let btc_account_balance = account_balances
            .get(&self.btc_id.to_string())
            .unwrap_or(&0)
            .to_owned();

        let market_balances = self
            .market
            .account(self.wallet.address().into())
            .await
            .unwrap()
            .value
            .liquid;

        let usdc_market_balance = market_balances.quote;
        let btc_market_balance = market_balances.base;

        println!(
            "Account balances: {:?} ETH, {:?} USDC, {:?} BTC",
            format_to_readable_value(eth_account_balance, 9),
            format_to_readable_value(usdc_account_balance, 6),
            format_to_readable_value(btc_account_balance, 8)
        );
        println!(
            "Market balances: {:?} USDC, {:?} BTC\n",
            format_to_readable_value(usdc_market_balance, 6),
            format_to_readable_value(btc_market_balance, 8)
        );

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
        println!("buy_queue = {:?}", buy_queue.len());
        println!("sell_queue = {:?}", sell_queue.len());
        if buy_queue.len() > 0 && sell_queue.len() > 0 {
            let best_bid = buy_queue
                .peek()
                .map(|order| order.price)
                .unwrap_or_default();
            let best_ask = sell_queue
                .peek()
                .map(|Reverse(order)| order.price)
                .unwrap_or_default();
            println!(
                "Best bid/ask = {:?} / {:?}",
                format_to_readable_value(best_bid.to_owned() as u64, 9),
                format_to_readable_value(best_ask.to_owned() as u64, 9)
            );
        }

        //todo выбирать цену изходя из ордербука
        let price: u64 = self
            .random
            .gen_range(60_000_000_000_000, 80_000_000_000_000)
            .await;
        let amount: u64 = self.random.gen_range(1_000_000, 10_000_000).await;

        let order_type = if self.random.gen_bool().await {
            OrderType::Buy
        } else {
            OrderType::Sell
        };

        if order_type == OrderType::Buy {
            self.check_and_prepare_balance_for_buy(
                price,
                amount,
                usdc_account_balance,
                usdc_market_balance,
            )
            .await
            .unwrap();
        } else {
            self.check_and_prepare_balance_for_sell(
                amount,
                btc_account_balance,
                btc_market_balance,
            )
            .await
            .unwrap();
        }

        // let action = self.random.gen_range(0, 3);
        let action = 2;
        let _res = match action {
            // 0 => self.immediate_or_cancel(order_type, price, amount).await,
            // 1 => self.fill_or_kill(order_type, price, amount).await,
            2 => self.good_till_cancelled(order_type, price, amount).await,
            _ => Ok(()),
        };
        self.order_manager.clear_orders().await;

        println!("------------------------------------------------\n");

        Ok(())
    }

    async fn check_and_prepare_balance_for_buy(
        &self,
        price: u64,
        amount: u64,
        usdc_account_balance: u64,
        usdc_market_balance: u64,
    ) -> Result<(), ()> {
        let total =
            (format_to_readable_value(price, 9) * format_to_readable_value(amount, 8) * 1e6) as u64;

        if usdc_market_balance >= total {
        } else if usdc_account_balance >= total {
            self.deposit(self.usdc_id, total).await?;
            println!("Deposit {} USDC", format_to_readable_value(total, 6));
        } else {
            // self.mint(self.usdc_id, total).await?;
            // self.deposit(self.usdc_id, total).await?;
            println!("Need to mint more USDC");
            return Err(());
        }

        Ok(())
    }

    async fn check_and_prepare_balance_for_sell(
        &self,
        amount: u64,
        btc_account_balance: u64,
        btc_market_balance: u64,
    ) -> Result<(), ()> {
        if btc_market_balance >= amount {
        } else if btc_account_balance >= amount {
            self.deposit(self.btc_id, amount).await?;
            println!("Deposit {} BTC", format_to_readable_value(amount, 8));
        } else {
            // self.mint(self.btc_id, amount).await?;
            // self.deposit(self.btc_id, amount).await?;
            println!("Need to mint more BTC");
            return Err(());
        }

        Ok(())
    }

    async fn good_till_cancelled(
        &self,
        order_type: OrderType,
        price: u64,
        amount: u64,
    ) -> Result<(), ()> {
        println!(
            "Open 'Good Till Cancelled' order: type = {:?}, price = {}, amount = {}",
            order_type,
            format_to_readable_value(price, 9),
            format_to_readable_value(amount, 8)
        );

        match self.market.open_order(amount, order_type, price).await {
            Ok(res) => {
                println!("Order id: 0x{}", Address::from(res.value.0).to_string());
                Ok(())
            }
            Err(e) => {
                println!("Cannot open order: {:?}", e);
                Err(())
            }
        }
    }

    async fn deposit(&self, asset_id: AssetId, amount: u64) -> Result<(), ()> {
        match self.market.deposit(amount, asset_id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("Deposit error: {:?}", e);
                Err(())
            }
        }
    }
}

impl OrderManager {
    pub async fn _get_all_orders(&self) -> (Vec<SpotOrder>, Vec<SpotOrder>) {
        let buy_orders = self.buy_orders.read().await;
        let sell_orders = self.sell_orders.read().await;

        let buy_list = buy_orders.values().flat_map(|v| v.clone()).collect();
        let sell_list = sell_orders.values().flat_map(|v| v.clone()).collect();

        (buy_list, sell_list)
    }
}

// pub async fn _mm(&self) -> Result<(), Error> {
//     info!("-----Trying to mm");

//     let mut buy_queue = BinaryHeap::new();
//     let mut sell_queue = BinaryHeap::new();

//     {
//         let buy_orders = self.order_manager.buy_orders.read().await;
//         for (_, orders) in buy_orders.iter() {
//             for order in orders {
//                 buy_queue.push(order.clone());
//             }
//         }

//         println!("{:?}", buy_orders);
//         let sell_orders = self.order_manager.sell_orders.read().await;
//         for (_, orders) in sell_orders.iter() {
//             for order in orders {
//                 sell_queue.push(Reverse(order.clone()));
//             }
//         }
//         println!("{:?}", sell_orders);
//     }

//     self.order_manager.clear_orders().await;

//     Ok(())
// }

// async fn immediate_or_cancel(
//     &self,
//     order_type: OrderType,
//     price: u64,
//     amount: u64,
// ) -> Result<(), ()> {
//     println!(
//         "Открытие ордера 'Immediate or Cancel': тип = {:?}, цена = {}, количество = {}\n",
//         order_type,
//         format_to_readable_value(price, 9),
//         format_to_readable_value(amount, 8)
//     );
//     // match self.market.open_order(amount, order_type, price).await {
//     //     Ok(_) => {
//     //         println!("Открытие ордера успешно");
//     //         Ok(())
//     //     }
//     //     Err(e) => {
//     let e = "Not implemented";
//     println!("Ошибка при открытии ордера: {:?}\n", e);
//     Err(())
//     //     }
//     // }
// }

// async fn fill_or_kill(&self, order_type: OrderType, price: u64, amount: u64) -> Result<(), ()> {
//     println!(
//         "Открытие ордера 'Fill or Kill': тип = {:?}, цена = {}, количество = {}\n",
//         order_type,
//         format_to_readable_value(price, 9),
//         format_to_readable_value(amount, 8)
//     );
//     // match self.market.open_order(amount, order_type, price).await {
//     //     Ok(_) => {
//     //         println!("Открытие ордера успешно");
//     //         Ok(())
//     //     }
//     //     Err(e) => {
//     let e = "Not implemented";
//     println!("Ошибка при открытии ордера: {:?}\n", e);
//     Err(())
//     //     }
//     // }
// }

// async fn post_only(&self, order_type: OrderType, price: u64, amount: u64) -> Result<(), ()> {
//     println!(
//         "Открытие ордера 'Post Only': тип = {:?}, цена = {}, количество = {}\n",
//         order_type,
//         format_to_readable_value(price, 9),
//         format_to_readable_value(amount, 8)
//     );
//     // match self.market.open_order(amount, order_type, price).await {
//     //     Ok(_) => {
//     //         println!("Открытие ордера успешно");
//     //         Ok(())
//     //     }
//     //     Err(e) => {
//     let e = "Not implemented";
//     println!("Ошибка при открытии ордера: {:?}\n", e);
//     Err(())
//     //     }
//     // }
// }

// async fn market_order(&self, order_type: OrderType, price: u64, amount: u64) -> Result<(), ()> {
//     println!(
//         "Открытие рыночного ордера: тип = {:?}, цена = {}, количество = {}\n",
//         order_type,
//         format_to_readable_value(price, 9),
//         format_to_readable_value(amount, 8)
//     );
//     // match self.market.open_order(amount, order_type, price).await {
//     //     Ok(_) => {
//     //         println!("Открытие ордера успешно");
//     //         Ok(())
//     //     }
//     //     Err(e) => {
//     let e = "Not implemented";
//     println!("Ошибка при открытии ордера: {:?}\n", e);
//     Err(())
//     //     }
//     // }
// }
// async fn mint(&self, asset_id: AssetId, amount: u64) -> Result<(), ()> {
//     // Логика минтинга
//     println!(
//         "Минтинг {}: {}\n",
//         asset_id,
//         format_to_readable_value(amount, 6)
//     );
//     let e = "Not implemented";
//     println!("Ошибка Минтинга: {:?}\n", e);
//     Err(())
// }

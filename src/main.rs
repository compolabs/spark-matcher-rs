use anyhow::Result;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use orderbook::{
    constants::{RPC, TOKEN_CONTRACT_ID},
    orderbook_utils::Orderbook,
    print_title,
};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{cmp::Ordering, collections::HashMap};

use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::{Bits256, ContractId},
};
use src20_sdk::token_utils::TokenContract;

use dotenv::dotenv;

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}

#[derive(Debug, Deserialize)]
pub struct IndexerOrder {
    pub id: usize,
    pub order_id: String,
    pub trader: String,
    pub base_token: String,
    pub base_size: String,
    pub base_price: String,
    pub timestamp: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

pub struct SparkMatcher {
    wallet: WalletUnlocked,
    token_contract: TokenContract<WalletUnlocked>,
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

        Ok(Self {
            wallet: wallet.clone(),
            token_contract: TokenContract::new(
                &ContractId::from_str(TOKEN_CONTRACT_ID).unwrap().into(),
                wallet.clone(),
            ),
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
                    println!("An error occurred while matching: `{}`", e);
                    tokio::time::sleep(Duration::from_millis(5000)).await;
                }
            }

            self.status = Status::Chill;
        }
    }

    fn format_indexer_url(order_type: OrderType) -> String {
        let order_type_str = format!(
            "&orderType={}",
            match order_type {
                OrderType::Sell => "sell",
                OrderType::Buy => "buy",
            }
        );
        format!(
            "{}{}",
            ev("INDEXER_URL_NO_ORDER_TYPE").unwrap_or("<ERROR>".to_owned()),
            order_type_str
        )
    }

    pub async fn fetch_orders_from_indexer(order_type: OrderType) -> Result<Vec<IndexerOrder>> {
        let indexer_url = Self::format_indexer_url(order_type);

        let sort_func: for<'a, 'b> fn(&'a IndexerOrder, &'b IndexerOrder) -> Ordering =
            match order_type {
                OrderType::Buy => |a, b| b.base_price.cmp(&a.base_price),
                OrderType::Sell => |a, b| a.base_price.cmp(&b.base_price),
            };

        let mut orders_deserialized = Client::new()
            .get(indexer_url)
            .send()
            .await?
            .json::<Vec<IndexerOrder>>()
            .await?;
        orders_deserialized.sort_by(sort_func);

        Ok(orders_deserialized)
    }

    async fn do_match(&mut self) -> Result<()> {
        let (mut sell_orders, mut buy_orders) = (
            Self::fetch_orders_from_indexer(OrderType::Sell).await?,
            Self::fetch_orders_from_indexer(OrderType::Buy).await?,
        );

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

                if sell_price <= buy_price
                    && sell_size < 0
                    && buy_size > 0
                    && sell_order.base_token == buy_order.base_token
                {
                    let sell_id = Bits256::from_hex_str(&sell_order.order_id)?;
                    let buy_id = Bits256::from_hex_str(&buy_order.order_id)?;

                    match self.orderbook.match_orders(&sell_id, &buy_id).await {
                        Ok(_) => println!(
                            "✅ Orders matched: sell => `{}`, buy => `{}`!\n",
                            &sell_order.order_id, &buy_order.order_id
                        ),
                        Err(e) => {
                            println!("matching error `{}`", e);
                            let sell_fail =
                                self.fails.entry(sell_order.order_id.clone()).or_insert(0);
                            *sell_fail += 1;

                            let buy_fail =
                                self.fails.entry(buy_order.order_id.clone()).or_insert(0);
                            *buy_fail += 1;
                        }
                    };
                }
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    print_title("Spark's Rust Matcher");
    dotenv().ok();

    let matcher = SparkMatcher::init().await?;
    let matcher_clone = matcher.clone();
    tokio::spawn(async move {
        let mut locked_matcher = matcher_clone.lock().await;
        locked_matcher.run().await;
    });

    let app = axum::Router::new()
        .route("/", get(echo_ok))
        .with_state(matcher.clone());
    let server_addr = format!(
        "localhost:{}",
        std::env::var("PORT").unwrap_or(5000.to_string())
    );
    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    println!("🚀 Server ready at: http://{}", &server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn echo_ok() -> impl IntoResponse {
    "Server is alive 👌"
}

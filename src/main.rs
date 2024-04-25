use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use orderbook::{
    constants::{ORDERBOOK_CONTRACT_ID, RPC, TOKEN_CONTRACT_ID},
    orderbook_utils::Orderbook,
    print_title,
};
use tokio::sync::Mutex;

use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use fuels::{
    crypto::SecretKey,
    prelude::{Provider, WalletUnlocked},
    types::ContractId,
};
use src20_sdk::token_utils::{Asset, TokenContract};

use dotenv::dotenv;

const MARKET_SYMBOL: &str = "BTC";
const BASE_SIZE: f64 = 0.01;
const BASE_PRICE: f64 = 65500.;

#[derive(Eq, PartialEq)]
pub enum Status {
    Chill,
    Active,
}

pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}

pub struct SparkMatcher {
    wallet: WalletUnlocked,
    token_contract: TokenContract<WalletUnlocked>,
    initialized: bool,
    status: Status,
    fails: HashMap<String, i64>,
}

impl SparkMatcher {
    pub async fn new() -> Result<Self> {
        let provider = Provider::connect(RPC).await?;
        let private_key = ev("PRIVATE_KEY")?;
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
        if !self.initialized {
            tokio::time::sleep(Duration::from_millis(1000)).await;

            Box::pin(self.process_next()).await;
            return;
        }

        if self.status == Status::Active {
            tokio::time::sleep(Duration::from_millis(1000)).await;

            Box::pin(self.process_next()).await;
            return;
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
        Box::pin(self.process_next()).await;
    }

    async fn do_match(&self) -> Result<()> {
        let token_contract_id = self.token_contract.contract_id().into();
        let base_asset = Asset::new(self.wallet.clone(), token_contract_id, MARKET_SYMBOL);
        let quote_asset = Asset::new(self.wallet.clone(), token_contract_id, "USDC");

        let orderbook = Orderbook::new(&self.wallet, ORDERBOOK_CONTRACT_ID).await;
        let price = (BASE_PRICE * 10f64.powf(orderbook.price_decimals as f64)) as u64;

        let base_size = base_asset.parse_units(BASE_SIZE as f64) as u64;
        base_asset
            .mint(self.wallet.address().into(), base_size)
            .await
            .unwrap();

        let sell_order_id = orderbook
            .open_order(base_asset.asset_id, -1 * base_size as i64, price - 1)
            .await
            .unwrap()
            .value;

        let quote_size = quote_asset.parse_units(BASE_SIZE as f64 * BASE_PRICE as f64);
        quote_asset
            .mint(self.wallet.address().into(), quote_size as u64)
            .await
            .unwrap();

        let buy_order_id = orderbook
            .open_order(base_asset.asset_id, base_size as i64, price)
            .await
            .unwrap()
            .value;

        println!(
            "buy_order = {:?}\n",
            orderbook
                .order_by_id(&buy_order_id)
                .await
                .unwrap()
                .value
                .unwrap()
        );
        println!(
            "sell_order = {:?}",
            orderbook
                .order_by_id(&sell_order_id)
                .await
                .unwrap()
                .value
                .unwrap()
        );

        orderbook
            .match_orders(&sell_order_id, &buy_order_id)
            .await
            .unwrap();

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

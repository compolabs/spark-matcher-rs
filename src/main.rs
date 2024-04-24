use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use fuels::accounts::wallet::WalletUnlocked;
use fuels::crypto::SecretKey;
use orderbook::constants::*;
use orderbook::print_title;

use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use dotenv::dotenv;
use fuels::accounts::provider::Provider;

pub enum Status {
    Chill,
    Active,
}

pub struct SparkMatcher {
    wallet: WalletUnlocked,
    initialized: bool,
    status: Status,
    fails: HashMap<String, i64>,
}

impl SparkMatcher {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let provider = Provider::connect(RPC).await?;
        let private_key = std::env::var("PRIVATE_KEY")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );

        Ok(Self {
            wallet: wallet.clone(),
            initialized: false,
            status: Status::Chill,
            fails: HashMap::new(),
        })
    }

    pub async fn init() -> Result<Arc<RwLock<Self>>, Box<dyn Error>> {
        Ok(Arc::new(RwLock::new(SparkMatcher::new().await?)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    print_title("Spark's Rust Matcher");
    dotenv().ok();
    let app = axum::Router::new()
        .route("/", get(echo_ok))
        .with_state(SparkMatcher::init().await?);
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

async fn do_match() {}

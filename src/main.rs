use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use fuels::accounts::wallet::WalletUnlocked;
use fuels::crypto::SecretKey;
use orderbook::constants::*;
use orderbook::print_title;
use tokio::sync::Mutex;

use anyhow::Result;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use dotenv::dotenv;
use fuels::accounts::provider::Provider;

#[derive(Eq, PartialEq)]
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
    pub async fn new() -> Result<Self> {
        let provider = Provider::connect(RPC).await?;
        let private_key = std::env::var("PRIVATE_KEY")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );

        Ok(Self {
            wallet: wallet.clone(),
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
                println!("An error occurred: `{}`", e);
                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }

        self.status = Status::Chill;
        Box::pin(self.process_next()).await;
    }

    async fn do_match(&self) -> Result<()> {
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

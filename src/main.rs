mod common;
mod log;
mod matcher;
mod websocket;

use std::sync::Arc;
use anyhow::Result;
use dotenv::dotenv;
use reqwest::Client;
use websocket::subscribe_orders;
use crate::matcher::*;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    log::setup_logging()?;

    let client = Client::new();
    let matcher = SparkMatcher::new(client.clone()).await?;
    let matcher = Arc::new(Mutex::new(matcher));

    tokio::spawn(async move {
        if let Err(e) = subscribe_orders().await {
            println!("WebSocket error: {}", e);
        }
    });

    matcher.lock().await.run().await;

    Ok(())
}

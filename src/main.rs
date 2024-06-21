mod common;
mod log;
mod matcher;

use ::log::info;
use anyhow::Result;

use dotenv::dotenv;
use reqwest::Client;

use crate::matcher::*;

#[tokio::main]
async fn main() -> Result<()> {
    // print_title("Spark's Rust Matcher");
    dotenv().ok();
    log::setup_logging()?;
    info!("Matcher launched, running...");

    let client = Client::new();
    let matcher = SparkMatcher::init(client).await?;
    let matcher_clone = matcher.clone();
    let mut locked_matcher = matcher_clone.lock().await;
    locked_matcher.run().await;

    Ok(())
}

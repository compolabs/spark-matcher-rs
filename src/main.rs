mod common;
mod matcher;
mod log;

use dotenv::dotenv;
use anyhow::Result;

use crate::matcher::SparkMatcher;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    log::setup_logging()?; 
    let ws_url = Url::parse(&common::ev("WEBSOCKET_URL")?)?;

    let matcher = SparkMatcher::new(ws_url).await?;
    let mut matcher_lock = matcher.lock().await;
    matcher_lock.run().await;

    Ok(())
}

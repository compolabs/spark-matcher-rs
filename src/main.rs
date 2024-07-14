mod api;
mod config;
mod market;
mod model;
mod util;
mod web;

use crate::market::SparkMatcher;
use anyhow::Result;
use dotenv::dotenv;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    util::logging::setup_logging()?;

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?;

    let matcher = SparkMatcher::new(ws_url).await?;
    let mut matcher_lock = matcher.lock().await;

    let _rocket = tokio::spawn(async {
        web::server::rocket().launch().await
    });

    matcher_lock.run().await;

    Ok(())
}

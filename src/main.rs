mod api;
mod config;
mod market;
mod model;
mod util;
mod web;

use crate::market::SparkMatcher;
use anyhow::{Result, Context};
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    util::logging::setup_logging()?;

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL").context("Failed to parse WEBSOCKET_URL")?)?;
    let matcher = SparkMatcher::new(ws_url).await.context("Failed to create SparkMatcher")?;
    let rocket = web::server::rocket(matcher);
    rocket.launch().await.context("Rocket failed to launch")?;

    Ok(())
}

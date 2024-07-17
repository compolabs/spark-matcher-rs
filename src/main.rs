mod api;
mod config;
mod market;
mod model;
mod util;
mod web;
mod error;

use crate::market::SparkMatcher;
use error::Error;
use url::Url;

#[tokio::main]
async fn main() -> Result<(),Error> {
    dotenv::dotenv().ok();

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?;
    let matcher = SparkMatcher::new(ws_url).await?;
    let rocket = web::server::rocket(matcher);
    rocket.launch().await?;

    Ok(())
}

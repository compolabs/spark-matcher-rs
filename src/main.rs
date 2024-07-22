mod api;
mod config;
mod error;
mod market;
mod management;
mod model;
mod util;
mod web;
mod websocket;

use std::sync::Arc;

use crate::market::SparkMatcher;
use error::Error;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?;
    let matcher = SparkMatcher::new(ws_url).await?;
    let matcher_clone = Arc::clone(&matcher);
    
    let matcher_task = tokio::spawn(async move {
        let mut lock = matcher_clone.lock().await;
        lock.run().await;
    });

    let rocket = web::server::rocket();
    let _rocket = rocket.launch().await?;

    matcher_task.await.expect("Matcher task failed");

    Ok(())
}

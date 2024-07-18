mod api;
mod config;
mod market;
mod model;
mod util;
mod web;
mod error;

use std::sync::Arc;

use crate::market::SparkMatcher;
use error::Error;
use tokio::sync::RwLock;
use url::Url;

#[tokio::main]
async fn main() -> Result<(),Error> {
    dotenv::dotenv().ok();

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?;
    let matcher = SparkMatcher::new(ws_url).await?;

    // Клонируем Arc для передачи в Rocket и отдельную асинхронную задачу
    let matcher_clone = matcher.clone();

    // Запускаем задачу для матчера в фоновом режиме
    tokio::spawn(async move {
        let mut matcher_guard = matcher_clone.lock().await;
        matcher_guard.run().await;
    });

    // Запускаем сервер Rocket
    let rocket = web::server::rocket(matcher);
    rocket.launch().await?;

    Ok(())
}

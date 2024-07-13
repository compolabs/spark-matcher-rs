mod api;
mod config;
mod market;
mod model;
mod util;

use crate::market::SparkMatcher;
use anyhow::Result;
use dotenv::dotenv;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    util::logging::setup_logging()?;

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?; // Получение URL WebSocket из переменных окружения

    let matcher = SparkMatcher::new(ws_url).await?; // Создание и инициализация матчера
    let mut matcher_lock = matcher.lock().await;
    matcher_lock.run().await; // Запуск основной логики матчера

    Ok(())
}

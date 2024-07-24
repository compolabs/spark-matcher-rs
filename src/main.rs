use market::SparkMatcher;
use tokio::signal;
use tokio::sync::mpsc;

mod api;
mod config;
mod error;
mod logger;
mod management;
mod market;
mod model;
mod util;
mod websocket;
mod web;

use management::manager::OrderManager;
use websocket::client::WebSocketClient;
use crate::error::Error;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();

    let ws_url = Url::parse(&config::ev("WEBSOCKET_URL")?)?;

    let websocket_client = WebSocketClient::new(ws_url);
    let order_manager = OrderManager::new();
    let arc_order_manager = order_manager.clone();

    let spark_matcher = SparkMatcher::new(arc_order_manager.clone()).await?;

    let (tx, mut rx) = mpsc::channel(100);

    let ws_task = tokio::spawn(async move {
        if let Err(e) = websocket_client.connect(tx).await {
            eprintln!("WebSocket error: {}", e);
        }
    });

    let manager_task = tokio::spawn(async move {
        while let Some(order) = rx.recv().await {
            order_manager.add_order(order).await;
        }
    });

    let matcher_task = tokio::spawn(async move {
        if let Err(e) = spark_matcher.run().await {
            eprintln!("SparkMatcher error: {}", e);
        }
    });

    let rocket_task = tokio::spawn(async {
        let rocket = web::server::rocket();
        let _ = rocket.launch().await;
    });

    let ctrl_c_task = tokio::spawn(async {
        signal::ctrl_c().await.expect("failed to listen for event");
        println!("Ctrl+C received!");
    });

    let _ = tokio::select! {
        _ = ws_task => { println!("WebSocket task finished"); },
        _ = manager_task => { println!("Order manager task finished"); },
        _ = rocket_task => { println!("Rocket server task finished"); },
        _ = ctrl_c_task => { println!("Shutting down..."); },
    };

    println!("Application is shutting down.");
    Ok(())
}

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

pub async fn subscribe_orders() -> Result<()> {
    let url = "wss://indexer.bigdevenergy.link/67b693c/v1/graphql";
    let (mut ws_stream, _) = connect_async(url)
        .await
        .context("Failed to connect to WebSocket")?;
    println!("WebSocket connected");

    let query = json!({
        "type": "start",
        "id": "1",
        "payload": {
            "query": r#"
                subscription MySubscription {
                    Order(
                        where: {status: {_gt: "Active"}, order_type: {_eq: "Sell"}},
                        order_by: {price: asc},
                        limit: 10
                    ) {
                        id
                        amount
                        asset
                        order_type
                        price
                        timestamp
                        status
                        user
                    }
                }
            "#
        }
    });

    ws_stream
        .send(Message::Text(query.to_string()))
        .await
        .context("Failed to send message")?;

    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("Received: {}", text);
            }
            Ok(Message::Close(_)) => {
                println!("Connection closed");
                break;
            }
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

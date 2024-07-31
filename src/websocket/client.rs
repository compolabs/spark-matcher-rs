use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::mpsc, time::{self, Duration}};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream};
use url::Url;
use log::{info, error};
use futures_util::SinkExt;

use crate::{api::subscription::format_graphql_subscription, model::{spot_order::{SpotOrderIndexer, WebSocketResponse}, OrderType, SpotOrder}};

pub struct WebSocketClient {
    pub url: Url,
}

impl WebSocketClient {
    pub fn new(url: Url) -> Self {
        WebSocketClient { url }
    }

    pub async fn connect(&self, sender: mpsc::Sender<SpotOrder>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Establishing websocket connection to {}", self.url);
        
        loop {
            let mut ws_stream = match self.connect_to_ws().await { //======== Изменен вызов подключения
                Ok(ws_stream) => ws_stream,
                Err(e) => {
                    error!("Failed to establish websocket connection: {:?}", e);
                    return Err(e);
                }
            };

            let mut timeout = time::interval(Duration::from_secs(300)); //======== Добавлен таймер таймаута

            while let Some(message) = ws_stream.next().await {
                tokio::select! {
                    _ = timeout.tick() => {
                        error!("No messages received in the last 5 minutes, reconnecting...");
                        break; // Выходим из цикла для переподключения
                    }
                    else => match message {
                        Ok(Message::Text(text)) => {
                            if let Ok(response) = serde_json::from_str::<WebSocketResponse>(&text) {
                                if response.r#type == "ka" {
                                    info!("Received keep-alive message.");
                                    continue;
                                } else if response.r#type == "connection_ack" {
                                    info!("Connection established, subscribing to orders...");
                                    self.subscribe_to_orders(OrderType::Buy, &mut ws_stream).await?;
                                    self.subscribe_to_orders(OrderType::Sell, &mut ws_stream).await?;
                                    continue;
                                } else if response.r#type == "data" {
                                    if let Some(payload) = response.payload {
                                        for order_indexer in payload.data.Order {
                                            let spot_order = SpotOrder::from_indexer(order_indexer)?;
                                            sender.send(spot_order).await?;
                                        }
                                    }
                                    timeout.reset(); //======== Сброс таймера при получении полезных данных
                                }
                            } else {
                                error!("Failed to deserialize WebSocketResponse: {:?}", text);
                            }
                        },
                        Ok(_) => continue,
                        Err(e) => {
                            error!("Error in websocket connection: {:?}", e);
                            break; // Выход из цикла для переподключения
                        }
                    }
                }
            }
        }
    }

    async fn connect_to_ws(&self) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Box<dyn std::error::Error>> { //======== Новый метод для упрощения переподключения
        match connect_async(&self.url).await {
            Ok((ws_stream, response)) => {
                info!("WebSocket handshake has been successfully completed with response: {:?}", response);
                Ok(ws_stream)
            },
            Err(e) => {
                error!("Failed to establish websocket connection: {:?}", e);
                Err(Box::new(e))
            }
        }
    }

    async fn subscribe_to_orders(
            &self,
            order_type: OrderType,
            client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let subscription_query = format_graphql_subscription(order_type);
            let start_msg = serde_json::json!({
                "id": format!("{}", order_type as u8),
                "type": "start",
                "payload": {
                    "query": subscription_query
                }
            })
            .to_string();
            client.send(Message::Text(start_msg)).await.map_err(|e| {
                error!("Failed to send subscription: {:?}", e);
                Box::new(e)
            })?;
            Ok(())
        }
}

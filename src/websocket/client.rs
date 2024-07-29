use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::mpsc};
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

    pub async fn connect(&self, sender: mpsc::Sender<(Vec<SpotOrder>,Vec<SpotOrder>)>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Establishing websocket connection to {}", self.url);
        let (mut ws_stream, _) = match connect_async(&self.url).await {
            Ok((ws_stream, response)) => {
                info!("WebSocket handshake has been successfully completed with response: {:?}", response);
                (ws_stream, response)
            }
            Err(e) => {
                error!("Failed to establish websocket connection: {:?}", e);
                return Err(Box::new(e));
            }
        };
        info!("WebSocket connected");

        ws_stream.send(Message::Text(r#"{"type": "connection_init"}"#.into())).await.expect("Failed to send init message");

        let mut initialized = false;

        self.subscribe_to_orders(OrderType::Buy, &mut ws_stream).await?;
        self.subscribe_to_orders(OrderType::Sell, &mut ws_stream).await?;

        while let Some(message) = ws_stream.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Ok(response) = serde_json::from_str::<WebSocketResponse>(&text) {
                        match response.r#type.as_str() {
                            "ka" => info!("Received keep-alive message."),
                            "connection_ack" => {
                                if !initialized {
                                    info!("Connection established, subscribing to orders...");
                                    self.subscribe_to_orders(OrderType::Buy, &mut ws_stream).await?;
                                    self.subscribe_to_orders(OrderType::Sell, &mut ws_stream).await?;
                                    initialized = true;
                                }
                            },
                            "data" => {
                                if let Some(payload) = response.payload {
                                    let mut new_buy_orders = Vec::new();
                                    let mut new_sell_orders = Vec::new();
                                    for order_indexer in payload.data.Order {
                                        let spot_order = SpotOrder::from_indexer(order_indexer)?;
                                        match spot_order.order_type {
                                            OrderType::Buy => new_buy_orders.push(spot_order),
                                            OrderType::Sell => new_sell_orders.push(spot_order),
                                        }
                                    }
                                    sender.send((new_buy_orders, new_sell_orders)).await?;
                                }
                            },
                            _ => error!("Unexpected response type: {}", response.r#type),
                        }
                    } else {
                        error!("Failed to deserialize WebSocketResponse: {:?}", text);
                    }
                },
                Ok(_) => continue,
                Err(e) => {
                    error!("Error in websocket connection: {:?}", e);
                    break;
                }
            }
        }

        Ok(())
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

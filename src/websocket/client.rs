use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use tokio::{
    net::TcpStream,
    sync::mpsc,
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use url::Url;

use crate::{
    api::subscription::format_graphql_subscription,
    model::{spot_order::WebSocketResponse, OrderType, SpotOrder},
};

pub struct WebSocketClient {
    pub url: Url,
}

impl WebSocketClient {
    pub fn new(url: Url) -> Self {
        WebSocketClient { url }
    }

    pub async fn connect(
        &self,
        sender: mpsc::Sender<SpotOrder>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let mut initialized = false;
            let mut ws_stream = match self.connect_to_ws().await {
                Ok(ws_stream) => ws_stream,
                Err(e) => {
                    error!("Failed to establish websocket connection: {:?}", e);
                    continue;
                }
            };

            info!("WebSocket connected");

            ws_stream
                .send(Message::Text(r#"{"type": "connection_init"}"#.into()))
                .await
                .expect("Failed to send init message");

            self.subscribe_to_orders(OrderType::Buy, &mut ws_stream)
                .await?;
            self.subscribe_to_orders(OrderType::Sell, &mut ws_stream)
                .await?;

            let mut last_data_time = Instant::now();
            while let Some(message) = ws_stream.next().await {
                if Instant::now().duration_since(last_data_time) > Duration::from_secs(60) {
                    error!("No data messages received for the last 60 seconds, reconnecting...");
                    break;
                }
                match message {
                    Ok(Message::Text(text)) => {
                        if let Ok(response) = serde_json::from_str::<WebSocketResponse>(&text) {
                            match response.r#type.as_str() {
                                "ka" => {
                                    info!("Received keep-alive message.");
                                    let b = Instant::now().duration_since(last_data_time);
                                    info!("time from last data: {:?}", b);
                                    continue;
                                }
                                "connection_ack" => {
                                    if !initialized {
                                        info!("Connection established, subscribing to orders...");
                                        self.subscribe_to_orders(OrderType::Buy, &mut ws_stream)
                                            .await?;
                                        self.subscribe_to_orders(OrderType::Sell, &mut ws_stream)
                                            .await?;
                                        initialized = true;
                                    }
                                }
                                "data" => {
                                    if let Some(payload) = response.payload {
                                        for order_indexer in payload.data.Order {
                                            let spot_order =
                                                SpotOrder::from_indexer(order_indexer)?;
                                            sender.send(spot_order).await?;
                                        }
                                        last_data_time = Instant::now();
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            error!("Failed to deserialize WebSocketResponse: {:?}", text);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error in websocket connection: {:?}", e);
                        break;
                    }
                }
            }

            self.unsubscribe_orders(&mut ws_stream, OrderType::Buy)
                .await?;
            self.unsubscribe_orders(&mut ws_stream, OrderType::Sell)
                .await?;
        }
    }

    async fn connect_to_ws(
        &self,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Box<dyn std::error::Error>> {
        match connect_async(&self.url).await {
            Ok((ws_stream, response)) => {
                info!(
                    "WebSocket handshake has been successfully completed with response: {:?}",
                    response
                );
                Ok(ws_stream)
            }
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

    async fn unsubscribe_orders(
        &self,
        client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
        order_type: OrderType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stop_msg = serde_json::json!({
            "id": format!("{}", order_type as u8),
            "type": "stop"
        })
        .to_string();
        client.send(Message::Text(stop_msg)).await.map_err(|e| {
            error!("Failed to send unsubscribe message: {:?}", e);
            Box::new(e)
        })?;
        Ok(())
    }
}

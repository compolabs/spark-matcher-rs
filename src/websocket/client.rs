use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;
use log::{info, error};

use crate::model::SpotOrder;

pub struct WebSocketClient {
    pub url: Url,
}

impl WebSocketClient {
    pub fn new(url: Url) -> Self {
        WebSocketClient { url }
    }

    pub async fn connect(&self, sender: mpsc::Sender<SpotOrder>) -> Result<(), Box<dyn std::error::Error>> {
            let (ws_stream, _) = connect_async(&self.url).await?;
            let (_, mut read) = ws_stream.split();

            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<SpotOrder>(&text) {
                            Ok(order) => sender.send(order).await?,
                            Err(e) => eprintln!("Failed to deserialize SpotOrder: {}", e),
                        }
                    }
                    Ok(_) => continue, // Пропускаем все не текстовые сообщения
                    Err(e) => {
                        error!("Error in websocket connection: {:?}", e);
                        break;
                    }
                }
            }

            Ok(())
        }
}

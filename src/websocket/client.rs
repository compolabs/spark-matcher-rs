use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use tokio::{
    net::TcpStream,
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use url::Url;

pub type WebSocketConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect_to_ws(url: &Url) -> Result<WebSocketConnection, Box<dyn std::error::Error>> {
    match tokio_tungstenite::connect_async(url).await {
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

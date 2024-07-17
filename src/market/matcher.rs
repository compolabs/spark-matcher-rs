use fuels::types::{Bits256, Bytes32};
use fuels::{
    accounts::provider::Provider,
    accounts::wallet::WalletUnlocked,
    crypto::SecretKey,
    types::ContractId,
};
use std::cmp::Ordering;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::Value;
use spark_market_sdk::MarketContract;
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

use crate::config::ev;
use crate::error::Error;
use crate::model::{OrderType, SpotOrder};
use crate::api::subscription::format_graphql_subscription;

pub struct MatcherState {
    pub buy_orders: Vec<SpotOrder>,
    pub sell_orders: Vec<SpotOrder>,
    pub market: MarketContract,
    pub active: bool
}

impl MatcherState {
    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

pub struct SparkMatcher {
    pub state: Arc<Mutex<MatcherState>>,
    pub client: Arc<Mutex<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>,
}

impl SparkMatcher {
    pub async fn new(ws_url: Url) -> Result<Arc<Mutex<Self>>,Error> {
        info!("Attempting to connect to WebSocket at: {}", ws_url);
        let (socket, _) = connect_async(&ws_url).await.map_err(Error::WebSocketConnectionError)?;
        info!("WebSocket connection established.");
        let provider = Provider::connect("testnet.fuel.network").await?;
        info!("Blockchain provider connected.");
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let secret_key = match SecretKey::from_str(&private_key) {
            Ok(sk) => sk,
            Err(_) => {
                return Err(Error::FuelCryptoPrivParseError);
            },
        };

        let wallet = WalletUnlocked::new_from_private_key(
            secret_key,
            Some(provider.clone()),
        );
        info!("Wallet created and connected to contract.");

        let market = MarketContract::new(ContractId::from_str(&contract_id)?, wallet).await;
        info!("Market contract initialized.");

        let state = MatcherState {
            buy_orders: Vec::new(),
            sell_orders: Vec::new(),
            market,
            active: true,
        };

        Ok(Arc::new(Mutex::new(Self {
            state: Arc::new(Mutex::new(state)),
            client: Arc::new(Mutex::new(socket)),
        })))
    }

    pub async fn run(&mut self) {
        let mut client = self.client.lock().await;

        client.send(Message::Text(r#"{"type": "connection_init"}"#.into())).await.expect("Failed to send init message");

        let mut initialized = false;
        while let Some(message) = client.next().await {
            if self.state.lock().await.active {
                match message {
                    Ok(Message::Text(text)) => {
                        if text.contains("connection_ack") && !initialized {
                            info!("Connection established, subscribing to orders...");
                            self.subscribe_to_orders(OrderType::Buy, &mut client).await;
                            self.subscribe_to_orders(OrderType::Sell, &mut client).await;
                            initialized = true;
                        } else if text.contains("ka") {
                            info!("Keep-alive message received.");
                        } else {
                            self.process_message(&text).await;
                        }
                    },
                    Ok(_) => {},
                    Err(e) => {
                        error!("Error during receive: {:?}", e);
                        break;
                    }
                }
            }
        }
    }

    async fn subscribe_to_orders(
        &self,
        order_type: OrderType,
        client: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    ) {
        let subscription_query = format_graphql_subscription(order_type);
        let start_msg = serde_json::json!({
            "id": format!("{}", order_type as u8),
            "type": "start",
            "payload": {
                "query": subscription_query
            }
        })
        .to_string();
        client.send(Message::Text(start_msg)).await.expect("Failed to send subscription");
    }

    async fn process_message(&self, message: &str) -> Result<(),Error> {
        let data: Value = serde_json::from_str(message)?;
        if data["type"] == "data" {
            let mut state = self.state.lock().await;
            if let Some(array) = data["payload"]["data"]["Order"].as_array() {
                state.buy_orders.extend(array.iter().filter_map(|v| {
                    if v["order_type"] == "Buy" {
                        serde_json::from_value(v.clone()).ok()
                    } else {
                        None
                    }
                }));

                state.sell_orders.extend(array.iter().filter_map(|v| {
                    if v["order_type"] == "Sell" {
                        serde_json::from_value(v.clone()).ok()
                    } else {
                        None
                    }
                }));

                self.match_orders(&mut state).await
            } else {
                Err(Error::ProcessMessagePayloadError(data.to_string()))
            }
        } else {
            Ok(())
        }
    }

    async fn match_orders(&self, state: &mut MatcherState) -> Result<(),Error> {
        info!("Attempting to match orders...");

        state.buy_orders.sort_by(|a, b| compare_values(&a.price, &b.price, false));
        state.sell_orders.sort_by(|a, b| compare_values(&a.price, &b.price, true));

        let mut buy_index = 0;
        let mut sell_index = 0;
        let mut matches = Vec::new();

        while buy_index < state.buy_orders.len() && sell_index < state.sell_orders.len() {
            let buy_order = &mut state.buy_orders[buy_index];
            let sell_order = &mut state.sell_orders[sell_index];

            if buy_order.price.parse::<u128>()? >= sell_order.price.parse::<u128>()? {
                let buy_amount = buy_order.amount.parse::<u128>()?;
                let sell_amount = sell_order.amount.parse::<u128>()?;
                let match_amount = std::cmp::min(buy_amount, sell_amount);

                buy_order.amount = (buy_amount - match_amount).to_string();
                sell_order.amount = (sell_amount - match_amount).to_string();

                info!("Matched: Buy order {} with Sell order {}, Amount {}", buy_order.id, sell_order.id, match_amount);

                if match_amount > 0 {
                    matches.push((buy_order.id.clone(), sell_order.id.clone(), match_amount));
                }

                if buy_order.amount == "0" {
                    buy_index += 1;
                }
                if sell_order.amount == "0" {
                    sell_index += 1;
                }
            } else {
                sell_index += 1;
            }
        }

        if !matches.is_empty() {
            post_matched_orders(&matches, &state.market).await?;
        }


        state.buy_orders.retain(|order| parse_u128_or_log(&order.amount).map_or(false, |num| num > 0));
        state.sell_orders.retain(|order| parse_u128_or_log(&order.amount).map_or(false, |num| num > 0));

        Ok(())
    }

}

async fn post_matched_orders(matches: &[(String, String, u128)], market: &MarketContract) -> Result<(),Error> {
    info!("Posting matched orders to the blockchain...");

    let mut ids = Vec::new();
    for (buy_id, sell_id, _) in matches {
        let buy_bits = Bits256::from_hex_str(buy_id)?;
        ids.push(buy_bits);
        let sell_bits = Bits256::from_hex_str(sell_id)?;
        ids.push(sell_bits);
    }

    if ids.is_empty() {
        info!("No orders to post.");
        return Ok(());
    }
    let ids_len = &ids.len();
    info!("Attempting to post {} matched orders to the blockchain.", &ids_len); 

    match market.match_order_many(ids).await {
        Ok(result) => {
            info!("Successfully matched orders. Transaction ID: https://app.fuel.network/tx/0x{}/simple", result.tx_id.unwrap_or(Bytes32::zeroed()));
            info!("Total matched orders posted: {}", ids_len);
            Ok(())
        },
        Err(e) => {
            error!("Failed to match orders on the blockchain: {:?}", e);
            error!("Transaction reverted. Continuing with next batch of orders.");
            Ok(()) 
        }
    }

}

fn parse_u128_or_log(s: &str) -> Option<u128> {
    match s.parse::<u128>() {
        Ok(value) => Some(value),
        Err(e) => {
            error!("{}", Error::OrderAmountParseError(e.to_string()));
            None
        }
    }
}

fn safe_parse<T: FromStr>(s: &str) -> Result<T, ()> {
    s.parse::<T>().map_err(|_| {
        error!("Failed to parse value: {}", s);
        ()
    })
}



fn compare_values(a: &str, b: &str, ascending: bool) -> Ordering {
    let parsed_a = safe_parse::<u128>(a).unwrap_or(0);
    let parsed_b = safe_parse::<u128>(b).unwrap_or(0);

    match ascending {
        true => parsed_a.cmp(&parsed_b),
        false => parsed_b.cmp(&parsed_a),
    }
}

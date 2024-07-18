use fuels::types::{Bits256, Bytes32};
use fuels::{
    accounts::provider::Provider,
    accounts::wallet::WalletUnlocked,
    crypto::SecretKey,
    types::ContractId,
};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use std::cmp::Ordering;
use std::time::Duration;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::Value;
use spark_market_sdk::MarketContract;
use std::{str::FromStr, sync::Arc};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

use crate::config::ev;
use crate::error::Error;
use crate::model::{OrderType, SpotOrder};
use crate::api::subscription::format_graphql_subscription;

pub type WSStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct ControlState {
    pub active: bool,
}

pub struct MatcherState {
    pub buy_orders: Vec<SpotOrder>,
    pub sell_orders: Vec<SpotOrder>,
    pub market: MarketContract,
}

impl ControlState {
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

pub enum MatcherCommand {
    Start,
    Stop,
    Status,
}

pub struct SparkMatcher {
    pub control_state: Arc<RwLock<ControlState>>,
    pub matcher_state: Arc<RwLock<MatcherState>>,
    pub client: Option<Arc<Mutex<WSStream>>>,
    pub ws_url: Url,
    pub command_sender: mpsc::Sender<MatcherCommand>,
}

impl SparkMatcher {
    pub async fn new(ws_url: Url) -> Result<Arc<Self>,Error> {
        let (sender, receiver) = mpsc::channel(100);
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

        let control_state = ControlState { active: true };
        let matcher_state = MatcherState {
            buy_orders: Vec::new(),
            sell_orders: Vec::new(),
            market,
        };
        let matcher = Arc::new(Self {
            control_state: Arc::new(RwLock::new(control_state)),
            matcher_state: Arc::new(RwLock::new(matcher_state)),
            client: None,
            ws_url,
            command_sender: sender,
        });

        let matcher_clone = Arc::clone(&matcher);
        tokio::spawn(async move {
            matcher_clone.run(receiver).await;
        });

        Ok(matcher)
    }

    pub async fn reconnect(&mut self) -> Result<(), Error> {
        let (socket, _) = connect_async(&self.ws_url).await.map_err(Error::WebSocketConnectionError)?;
        info!("WebSocket connection established.");
        self.client = Some(Arc::new(Mutex::new(socket)));
        Ok(())
    }

    pub async fn run(self: Arc<Mutex<Self>>, mut receiver: mpsc::Receiver<MatcherCommand>) {
        let mut initialized = false;
        loop {
            tokio::select! {
                Some(command) = receiver.recv() => {
                    match command {
                        MatcherCommand::Start => {
                            let mut control_state = self.lock().await.control_state.write().await;
                            control_state.activate();
                            info!("Matcher activated.");
                        },
                        MatcherCommand::Stop => {
                            let mut control_state = self.lock().await.control_state.write().await;
                            control_state.deactivate();
                            info!("Matcher deactivated.");
                        },
                        MatcherCommand::Status => {
                            let control_state = self.lock().await.control_state.read().await;
                            info!("Matcher status: active={}", control_state.is_active());
                        },
                    }
                },
                _ = async {
                    let active = {
                        let control_state = self.lock().await.control_state.read().await;
                        control_state.is_active()
                    };

                    if active {
                        let mut need_reconnect = false;

                        if self.lock().await.client.is_none() {
                            if let Err(e) = self.lock().await.reconnect().await {
                                error!("Connection failed: {:?}", e);
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                return;
                            }
                        }

                        if let Some(client) = &self.lock().await.client {
                            let mut client_guard = client.lock().await;
                            while let Some(message_result) = client_guard.next().await {
                                match message_result {
                                    Ok(Message::Text(text)) => {
                                        self.lock().await.process_text_message(&text, &mut client_guard, &mut initialized).await;
                                    },
                                    Ok(_) => {},
                                    Err(e) => {
                                        error!("Error during receive: {:?}", e);
                                        need_reconnect = true;
                                        break;
                                    }
                                }

                                {
                                    let control_state = self.lock().await.control_state.read().await;
                                    if !control_state.is_active() {
                                        break;
                                    }
                                }
                            }
                        }

                        if need_reconnect {
                            self.lock().await.client = None;
                        }
                    }
                } => {}
            }
        }
    }

    async fn process_text_message(&self, text: &str, client: &mut WSStream, initialized: &mut bool) {
            if text.contains("connection_ack") && !*initialized {
                info!("Connection established, subscribing to orders...");
                self.subscribe_to_orders(OrderType::Buy, client).await;
                self.subscribe_to_orders(OrderType::Sell, client).await;
                *initialized = true;
            } else if text.contains("ka") {
                info!("Keep-alive message received.");
            } else {
                self.process_message(&text).await;
            }
        }

    async fn subscribe_to_orders(
        &self,
        order_type: OrderType,
        client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
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
            let mut state = self.state.write().await;
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

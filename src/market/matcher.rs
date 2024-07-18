use crate::config::ev;
use crate::model::{OrderType, SpotOrder};
use crate::api::subscription::format_graphql_subscription;
use anyhow::{Context, Result};
use fuels::types::Bits256;
use fuels::{
    accounts::provider::Provider,
    accounts::wallet::WalletUnlocked,
    crypto::SecretKey,
    types::ContractId,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use spark_market_sdk::MarketContract;
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

#[derive(Debug, Eq, PartialEq)]
pub enum Status {
    Chill,
}

pub struct MatcherState {
    buy_orders: Vec<SpotOrder>,
    sell_orders: Vec<SpotOrder>,
    market: MarketContract,
}

pub struct SparkMatcher {
    state: Arc<Mutex<MatcherState>>,
    client: Arc<Mutex<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>,
}

impl SparkMatcher {
    pub async fn new(ws_url: Url) -> Result<Arc<Mutex<Self>>> {
        let (socket, _) = connect_async(&ws_url).await.context("Failed to connect to WebSocket")?;
        let provider = Provider::connect("testnet.fuel.network").await?;
        let private_key = ev("PRIVATE_KEY")?;
        let contract_id = ev("CONTRACT_ID")?;
        let wallet = WalletUnlocked::new_from_private_key(
            SecretKey::from_str(&private_key)?,
            Some(provider.clone()),
        );
        let market = MarketContract::new(ContractId::from_str(&contract_id).unwrap(), wallet).await;

        let state = MatcherState {
            buy_orders: Vec::new(),
            sell_orders: Vec::new(),
            market,
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
            match message {
                Ok(Message::Text(text)) => {
                    if text.contains("connection_ack") && !initialized {
                        println!("Connection established, subscribing to orders...");
                        self.subscribe_to_orders(OrderType::Buy, &mut client).await;
                        self.subscribe_to_orders(OrderType::Sell, &mut client).await;
                        initialized = true;
                    } else if text.contains("ka") {
                        println!("Keep-alive message received.");
                    } else {
                        self.process_message(&text).await;
                    }
                },
                Ok(_) => {},
                Err(e) => {
                    eprintln!("Error during receive: {:?}", e);
                    break;
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

    async fn process_message(&self, message: &str) {
        let data: Value = serde_json::from_str(message).unwrap();
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

                self.match_orders(&mut state).await.expect("Failed to match orders");
            }
        }
    }

    async fn match_orders(&self, state: &mut MatcherState) -> Result<()> {
        println!("Attempting to match orders...");

        state.buy_orders.sort_by(|a, b| b.price.parse::<u128>().unwrap().cmp(&a.price.parse::<u128>().unwrap()));
        state.sell_orders.sort_by(|a, b| a.price.parse::<u128>().unwrap().cmp(&b.price.parse::<u128>().unwrap()));

        println!("-----------------------------------");
        println!("buy_orders:{:?}",state.buy_orders);
        println!("-----------------------------------");
        println!("sell_orders:{:?}",state.buy_orders);
        println!("-----------------------------------");

        let mut buy_index = 0;
        let mut sell_index = 0;
        let mut matches = Vec::new();

        while buy_index < state.buy_orders.len() && sell_index < state.sell_orders.len() {
            let buy_order = &mut state.buy_orders[buy_index];
            let sell_order = &mut state.sell_orders[sell_index];

            if buy_order.price.parse::<u128>().unwrap() >= sell_order.price.parse::<u128>().unwrap() {
                let buy_amount = buy_order.amount.parse::<u128>().unwrap();
                let sell_amount = sell_order.amount.parse::<u128>().unwrap();
                let match_amount = std::cmp::min(buy_amount, sell_amount);

                buy_order.amount = (buy_amount - match_amount).to_string();
                sell_order.amount = (sell_amount - match_amount).to_string();

                println!("Matched: Buy order {} with Sell order {}, Amount {}", buy_order.id, sell_order.id, match_amount);

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

        state.buy_orders.retain(|order| order.amount.parse::<u128>().unwrap() > 0);
        state.sell_orders.retain(|order| order.amount.parse::<u128>().unwrap() > 0);

        Ok(())
    }

}

async fn post_matched_orders(matches: &[(String, String, u128)], market: &MarketContract) -> Result<()> {
    println!("Posting matched orders to the blockchain...");

    let ids: Vec<Bits256> = matches.iter()
        .map(|(buy_id, sell_id, _)| {
            [buy_id, sell_id].iter().map(|id| Bits256::from_hex_str(id).unwrap()).collect::<Vec<_>>()
        })
        .flatten()
        .collect();

    if ids.is_empty() {
        println!("No orders to post.");
        return Ok(());
    }
    let ids_len = &ids.len();
    println!("Attempting to post {} matched orders to the blockchain.", &ids_len); 

    match market.match_order_many(ids).await {
        Ok(result) => {
            println!("Successfully matched orders. Transaction ID: https://app.fuel.network/tx/0x{}/simple", result.tx_id.unwrap());
            println!("Total matched orders posted: {}", ids_len);
            Ok(())
        },
        Err(e) => {
            println!("Failed to match orders on the blockchain: {:?}", e);
            println!("Transaction reverted. Continuing with next batch of orders.");
            Ok(()) 
        }
    }
}


use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use itertools::Itertools;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Deserialize)]
pub struct IndexerOrder {
    pub id: usize,
    pub order_id: String,
    pub trader: String,
    pub base_token: String,
    pub base_size: String,
    pub base_price: String,
    pub timestamp: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

pub fn ev(key: &str) -> Result<String> {
    std::env::var(key).map_err(|e| anyhow!("Failed to get env variable {}: {}", key, e))
}

struct Config {
    indexer_url: String,
    fetch_order_limit: String,
}

impl Config {
    fn new() -> Result<Self> {
        let indexer_url = ev("INDEXER_URL")?;
        let fetch_order_limit = ev("FETCH_ORDER_LIMIT").unwrap_or_else(|_| "1000".to_string());
        Ok(Self {
            indexer_url,
            fetch_order_limit,
        })
    }

    fn format_indexer_url(&self, order_type: OrderType) -> String {
        let order_type_str = match order_type {
            OrderType::Sell => "sell",
            OrderType::Buy => "buy",
        };
        format!(
            "{}/spot/orders?limit={}&isOpened=true&orderType={}",
            self.indexer_url, self.fetch_order_limit, order_type_str
        )
    }
}

pub async fn fetch_orders_from_indexer(order_type: OrderType) -> Result<Vec<IndexerOrder>> {
    let config = Config::new()?;
    let indexer_url = config.format_indexer_url(order_type);

    let response = Client::new()
        .get(&indexer_url)
        .send()
        .await?
        .error_for_status()?;

    let orders_deserialized: Vec<IndexerOrder> = response.json().await?;

    let orders_iter = orders_deserialized.into_iter();

    let orders = match order_type {
        OrderType::Buy => {
            orders_iter.sorted_by_key(|order| order.base_price.clone()).collect::<Vec<IndexerOrder>>()
        }
        OrderType::Sell => {
            orders_iter.sorted_by_key(|order| order.base_price.clone()).rev().collect::<Vec<IndexerOrder>>()
        }
    };

    Ok(orders)
}


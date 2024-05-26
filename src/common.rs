use ::log::debug;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

use std::cmp::Ordering;

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
    Ok(std::env::var(key)?)
}

fn format_indexer_url(order_type: OrderType) -> String {
    let order_count = ev("FETCH_ORDER_LIMIT").unwrap_or("1000".to_string());
    let order_type_str = format!(
        "/spot/orders?limit={}&isOpened=true&orderType={}",
        order_count,
        match order_type {
            OrderType::Sell => "sell",
            OrderType::Buy => "buy",
        }
    );
    let url = format!(
        "{}{}",
        ev("INDEXER_URL").unwrap_or("<ERROR>".to_owned()),
        order_type_str
    );
    // debug!("Final indexer URL is: `{}`", &url);
    url
}

pub async fn fetch_orders_from_indexer(order_type: OrderType) -> Result<Vec<IndexerOrder>> {
    let indexer_url = format_indexer_url(order_type);
    let sort_func: for<'a, 'b> fn(&'a IndexerOrder, &'b IndexerOrder) -> Ordering = match order_type
    {
        OrderType::Buy => |a, b| b.base_price.cmp(&a.base_price),
        OrderType::Sell => |a, b| a.base_price.cmp(&b.base_price),
    };
    let mut orders_deserialized = Client::new()
        .get(indexer_url)
        .send()
        .await?
        .json::<Vec<IndexerOrder>>()
        .await?;
    orders_deserialized.sort_by(sort_func);

    Ok(orders_deserialized)
}

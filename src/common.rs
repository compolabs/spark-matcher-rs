
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}
pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}
#[derive(Debug, Deserialize)]
pub struct IndexerOrder {
    pub id: String,
    pub base_price: String,
    pub base_size: String,
    pub base_token: String,
    pub db_write_timestamp: String,
    pub order_type: String,
    pub timestamp: String,
    pub trader: String,
}

pub async fn fetch_orders_from_indexer(order_type: OrderType) -> Result<Vec<IndexerOrder>> {
    let indexer_url = "http://your-hasura-endpoint.com/v1/graphql"; // Замените на правильный URL вашего GraphQL-эндпоинта
    let graphql_query = format!(
        r#"query MyQuery {{
            SpotOrder {{
                id
                base_price
                base_size
                base_token
                db_write_timestamp
                order_type
                timestamp
                trader
            }}
        }}"#
    );

    let client = Client::new();
    let response = client
        .post(indexer_url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"query": graphql_query}))
        .send()
        .await?;

    let body = response.text().await?;
    let result: serde_json::Value = serde_json::from_str(&body)?;

    let orders_deserialized = serde_json::from_value::<Vec<IndexerOrder>>(
        result["data"]["SpotOrder"].to_owned(),
    )?;

    let sort_func: fn(&IndexerOrder, &IndexerOrder) -> Ordering = match order_type {
        OrderType::Buy => |a, b| b.base_price.cmp(&a.base_price),
        OrderType::Sell => |a, b| a.base_price.cmp(&b.base_price),
    };

    let mut orders_deserialized = orders_deserialized;
    orders_deserialized.sort_by(sort_func);

    Ok(orders_deserialized)
}

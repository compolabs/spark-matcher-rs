use std::cmp::Ordering;

use ::log::debug;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct IndexerOrder {
    pub id: String,
    pub base_price: String,
    pub base_size: String,
    pub base_token: String,
    #[serde(rename = "db_write_timestamp")]
    pub db_write_timestamp: String,
    #[serde(deserialize_with = "deserialize_order_type")]
    pub order_type: SpotOrderType,
    pub timestamp: String,
    pub trader: String,
}

#[derive(Debug)]
pub enum SpotOrderType {
    buy,
    sell,
}

impl<'de> Deserialize<'de> for SpotOrderType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        match s {
            "sell" => Ok(SpotOrderType::sell),
            "buy" => Ok(SpotOrderType::buy),
            _ => Err(serde::de::Error::custom("invalid SpotOrderType")),
        }
    }
}

fn deserialize_order_type<'de, D>(deserializer: D) -> Result<SpotOrderType, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "sell" => Ok(SpotOrderType::sell),
        "buy" => Ok(SpotOrderType::buy),
        _ => Err(serde::de::Error::custom("invalid SpotOrderType")),
    }
}
pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}
pub async fn fetch_orders_from_indexer(order_type: SpotOrderType) -> Result<Vec<IndexerOrder>> {
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
    let indexer_url = ev("INDEXER_URL").unwrap_or("<ERROR>".to_owned());

    let client = Client::new();
    let response = client
        .post(indexer_url)
        .header("Content-Type", "application/json")
        .body(serde_json::json!({ "query": graphql_query }).to_string())
        .send()
        .await?;

    if response.status().is_success() {
        let json_body = response.text().await?;
        // println!("Response body: {:?}", json_body);

        let parsed_response: serde_json::Value = serde_json::from_str(&json_body)?;
        // println!("Parsed response: {:?}", parsed_response);

        let mut orders: Vec<IndexerOrder> =
            serde_json::from_value(parsed_response["data"]["SpotOrder"].clone())?;

        // Сортировка в зависимости от типа ордера
        let sort_func: fn(&IndexerOrder, &IndexerOrder) -> Ordering = match order_type {
            SpotOrderType::buy => |a, b| b.base_price.cmp(&a.base_price),
            SpotOrderType::sell => |a, b| a.base_price.cmp(&b.base_price),
        };
        orders.sort_by(sort_func);

        Ok(orders)
    } else {
        Err(anyhow::anyhow!(
            "Request failed with status: {}",
            response.status()
        ))
    }
}


use std::cmp::Ordering;

use ::log::debug;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct IndexerOrder {
    pub id: String,
    pub base_price: i64,
    pub base_size: i64,
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
    debug!("Indexer URL: {}", indexer_url);

    let client = Client::new();
    let response = client
        .post(&indexer_url)
        .header("Content-Type", "application/json")
        .body(serde_json::json!({ "query": graphql_query }).to_string())
        .send()
        .await?;

    debug!("Response status: {}", response.status());

    if response.status().is_success() {
        let json_body = response.text().await?;
        debug!("Full response body length: {}", json_body.len());

        let parsed_response: serde_json::Value = serde_json::from_str(&json_body)?;
        debug!("Full parsed response: {:?}", parsed_response);

        let orders_value = &parsed_response["data"]["SpotOrder"];
        let limited_orders: Vec<serde_json::Value> = orders_value
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .take(5)
            .cloned()
            .collect();
        debug!("Limited orders: {:?}", limited_orders);

        match serde_json::from_value::<Vec<IndexerOrder>>(orders_value.clone()) {
            Ok(mut orders) => {
                debug!("Parsed orders: {:?}", orders.iter().take(5).collect::<Vec<_>>());

                // Сортировка в зависимости от типа ордера
                let sort_func: fn(&IndexerOrder, &IndexerOrder) -> Ordering = match order_type {
                    SpotOrderType::buy => |a, b| b.base_price.cmp(&a.base_price),
                    SpotOrderType::sell => |a, b| a.base_price.cmp(&b.base_price),
                };
                orders.sort_by(sort_func);

                debug!("Sorted orders: {:?}", orders.iter().take(20).collect::<Vec<_>>());
                Ok(orders)
            }
            Err(e) => {
                debug!("Error deserializing orders: {:?}", e);
                Err(anyhow::anyhow!("Error deserializing orders: {:?}", e))
            }
        }
    } else {
        Err(anyhow::anyhow!(
            "Request failed with status: {}",
            response.status()
        ))
    }
}

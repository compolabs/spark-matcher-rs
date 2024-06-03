use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Deserialize)]
pub struct SpotOrder {
    pub id: String,
    pub trader: String,
    pub base_token: String,
    pub base_size: String,
    pub base_price: String,
    pub timestamp: String,
    pub order_type: String,
}

pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}

fn format_graphql_query(order_type: OrderType) -> String {
    let limit = ev("FETCH_ORDER_LIMIT").unwrap_or_else(|_| "100".to_string());
    let (order_type_str, order_by) = match order_type {
        OrderType::Sell => ("sell", "asc"),
        OrderType::Buy => ("buy", "desc"),
    };

    let query = json!({
        "query": format!(
            r#"
            query {{
                SpotOrder(
                    limit: {}, 
                    where: {{order_type: {{_eq: "{}"}}, base_size: {{_neq: "0"}}}}, 
                    order_by: {{base_price: {}}}
                ) {{
                        id
                        trader
                        timestamp
                        order_type
                        base_size
                        base_token
                        base_price
                }}
            }}"#,
            limit, order_type_str, order_by
        )
    });

    query.to_string()
}

pub async fn fetch_orders_from_indexer(order_type: OrderType) -> Result<Vec<SpotOrder>> {
    let graphql_query = format_graphql_query(order_type);
    let graphql_url = ev("INDEXER_URL").unwrap();

    let client = Client::new();
    let response = client
        .post(&graphql_url)
        .header("Content-Type", "application/json")
        .body(graphql_query)
        .send()
        .await?;

    if response.status().is_success() {
        let json_response: serde_json::Value = response.json().await?;
        let orders: Vec<SpotOrder> = serde_json::from_value(json_response["data"]["SpotOrder"].clone())
            .context("Failed to parse orders from response")?;
        Ok(orders)
    } else {
        Err(anyhow::anyhow!("Request failed with status: {}", response.status()))
    }
}
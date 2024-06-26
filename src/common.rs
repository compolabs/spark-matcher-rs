use anyhow::{Context, Result};
use log::error;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::time::Instant;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Deserialize)]
pub struct SpotOrder {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: String,
    pub price: String,
    pub timestamp: String,
    pub order_type: String,
}

pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}

fn format_graphql_query(order_type: OrderType) -> String {
    let limit = ev("FETCH_ORDER_LIMIT").unwrap_or_else(|_| "100".to_string());
    let (order_type_str, order_by) = match order_type {
        OrderType::Sell => ("Sell", "asc"),
        OrderType::Buy => ("Buy", "desc"),
    };

    let query = json!({
        "query": format!(
            r#"
            query {{
                Order(
                    limit: {}, 
                    where: {{ status: {{_eq: "Active"}}, order_type: {{_eq: "{}"}} }}, 
                    order_by: {{price: {}}},
                ) {{
                        id
                        user
                        timestamp
                        order_type
                        amount
                        asset
                        price
                }}
            }}"#,
            limit, order_type_str, order_by
        )
    });

    query.to_string()
}

pub async fn fetch_orders_from_indexer(
    order_type: OrderType,
    client: &Client,
) -> Result<Vec<SpotOrder>> {
    let start = Instant::now();
    let graphql_query = format_graphql_query(order_type);
    let graphql_url = ev("INDEXER_URL").unwrap();

    let response = client
        .post(&graphql_url)
        .header("Content-Type", "application/json")
        .body(graphql_query.to_string())
        .send()
        .await?;
    // println!(
    //     "graphql_query.to_string() = {:?}",
    //     graphql_query.to_string()
    // );
    if response.status().is_success() {
        let json_response: serde_json::Value = response.json().await?;
        let orders: Vec<SpotOrder> = serde_json::from_value(json_response["data"]["Order"].clone())
            .context("Failed to parse orders from response")?;

        // let duration = start.elapsed();
        // match order_type {
        //     OrderType::Sell => info!("fetch_orders_from_indexer(Sell) executed in {:?}", duration),
        //     OrderType::Buy => info!("fetch_orders_from_indexer(Buy) executed in {:?}", duration),
        // }
        Ok(orders)
    } else {
        let duration = start.elapsed();
        match order_type {
            OrderType::Sell => error!(
                "fetch_orders_from_indexer(Sell) failed in {:?} with status: {}",
                duration,
                response.status()
            ),
            OrderType::Buy => error!(
                "fetch_orders_from_indexer(Buy) failed in {:?} with status: {}",
                duration,
                response.status()
            ),
        }

        Err(anyhow::anyhow!(
            "Request failed with status: {}",
            response.status()
        ))
    }
}

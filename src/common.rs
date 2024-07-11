use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use std::env;

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
    env::var(key).context(format!("Environment variable {} not found", key))
}

pub fn format_graphql_subscription(order_type: OrderType) -> String {
    let limit = ev("FETCH_ORDER_LIMIT").unwrap_or_default();
    let (order_type_str, order_by) = match order_type {
        OrderType::Sell => ("Sell", "asc"),
        OrderType::Buy => ("Buy", "desc"),
    };

    format!(
        r#"subscription {{
            Order(
                limit: {}, 
                where: {{ status: {{_eq: "Active"}}, order_type: {{_eq: "{}"}} }}, 
                order_by: {{price: {}}}
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
}

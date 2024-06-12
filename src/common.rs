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

    //fixme
    let ignored_ids = vec![
        "0x994748766209d760fbd474d2d7f0539f387712bd27c4f4163c6beea28e537764",
        "0x74e6e3cbfe398ba162e94e96840a03d11935a9b9e39855ed012457f7af02c615",
        "0xa3705af71d075c3a0a377df886689f471a22d0c9b59ecfcdd832028202ec346e",
        "0x97ff915f6783fdba0ed03d8dc0b60f90186c08646ebbe0cd1e25863ee700478d",
        "0x0f4fcb1698d9b371d88d2969020cf3336de551bf132b976fad4f31d123f5dea3",
        "0x47629fb5c52d919a3cc092e5de04063bd19ba1e97c8ee10f57158203f08fc30b",
        "0x1eedf74e7946b69d104ae2fa29e3d29b3c5ff230ae33dd52f0fb59034f253383",
        "0xeff1f3a1e158c614c7f1a07afeb5ec2c3215facccedb14c2dfe76b7261d96fb2",
        "0xf8506cca112c7179790a53014b069a74452580437e6745f6d7466c9ca6637455",
        "0x3270ef338da4208042935796e9e3014be88b3ddd167b064bbc0f5c06f7a5fb9b",
        "0x1145f59a4cbbdb0a2d1d5178af57b0453aff7b2a40ab0460b8b1e6440204fd58",
        "0x6b83571981ce942ccf7f9b3b21c39d03cadb297ad7e8520bd37f62661de71890",
        "0x8fd909610f80f3f670b7f956cf244222ed352952505cac6f55144b1c20118b00",
        "0x572a2059b4af424f535c5169b0c3b3e24491c0df0b938ae62f99930fca50a607",
        "0xf533fe17191ebb346c760043f547afe138572ad7030b780623aa943fe0c732bf",
        "0x176dd6c63d90d501ee29e119299a962c34eb5bd1697777277e1a4046ee8fba1b",
        "0x5aa518e6eafe8e7da53aeceb54789342c142997bf93b0a96325b43299e7a683c",
    ]; 

    let ignored_ids_str = ignored_ids
        .iter()
        .map(|id| format!(r#""{}""#, id))
        .collect::<Vec<String>>()
        .join(", ");

    let query = json!({
        "query": format!(
            r#"
            query {{
                SpotOrder(
                    limit: {}, 
                    where: {{id: {{_nin: [{}]}},order_type: {{_eq: "{}"}}, base_size: {{_neq: "0"}}}}, 
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
            limit, ignored_ids_str,order_type_str, order_by
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
        let orders: Vec<SpotOrder> =
            serde_json::from_value(json_response["data"]["SpotOrder"].clone())
                .context("Failed to parse orders from response")?;
        Ok(orders)
    } else {
        Err(anyhow::anyhow!(
            "Request failed with status: {}",
            response.status()
        ))
    }
}

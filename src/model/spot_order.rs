use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Eq, Clone, Copy, JsonSchema, Serialize, Deserialize)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
pub struct SpotOrder {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: u128,
    pub price: u128,
    pub timestamp: u64,
    pub order_type: OrderType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpotOrderIndexer {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: String,
    pub price: String,
    pub timestamp: String,
    pub order_type: OrderType,
}

impl SpotOrder {
    pub fn from_indexer(intermediate: SpotOrderIndexer) -> Result<Self, Box<dyn std::error::Error>> {
        let amount = intermediate.amount.parse::<u128>()?;
        let price = intermediate.price.parse::<u128>()?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&intermediate.timestamp)?.timestamp() as u64;
        
        Ok(SpotOrder {
            id: intermediate.id,
            user: intermediate.user,
            asset: intermediate.asset,
            amount,
            price,
            timestamp,
            order_type: intermediate.order_type,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderPayload {
    pub Order: Vec<SpotOrderIndexer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataPayload {
    pub data: OrderPayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSocketResponse {
    pub r#type: String,
    pub id: Option<String>,
    pub payload: Option<DataPayload>,
}

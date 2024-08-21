use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Eq, Clone, Copy, JsonSchema, Serialize, Deserialize)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize, Eq)]
pub struct SpotOrder {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: u128,
    pub price: u128,
    pub timestamp: u64,
    pub order_type: OrderType,
}

impl PartialEq for SpotOrder {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl Ord for SpotOrder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price.cmp(&other.price)
    }
}

impl PartialOrd for SpotOrder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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
    pub status: Option<String>,          // Новое поле
    pub asset_type: Option<String>,      // Новое поле
    pub db_write_timestamp: Option<String>, // Новое поле
    pub initial_amount: Option<String>,  // Новое поле
}

impl SpotOrder {
    pub fn from_indexer(
        intermediate: SpotOrderIndexer,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let amount = intermediate.amount.parse::<u128>()?;
        let price = intermediate.price.parse::<u128>()?;
        let timestamp =
            chrono::DateTime::parse_from_rfc3339(&intermediate.timestamp)?.timestamp() as u64;

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
    #[serde(rename = "ActiveBuyOrder")]
    pub active_buy_order: Option<Vec<SpotOrderIndexer>>,
    
    #[serde(rename = "ActiveSellOrder")]
    pub active_sell_order: Option<Vec<SpotOrderIndexer>>,
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

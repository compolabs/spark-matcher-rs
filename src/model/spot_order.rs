use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Deserialize)]
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

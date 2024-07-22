use serde::{Deserialize, Deserializer};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpotOrder {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: u128,
    pub price: u128,
    pub timestamp: u64,
    pub order_type: OrderType,
}



impl<'de> Deserialize<'de> for OrderType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_ref() {
            "Buy" => Ok(OrderType::Buy),
            "Sell" => Ok(OrderType::Sell),
            _ => Err(serde::de::Error::custom("unknown order type")),
        }
    }
}

impl SpotOrder {
    pub fn new_from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Self>(json)
    }
}

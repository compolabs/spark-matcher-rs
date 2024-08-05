use std::env::VarError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to connect to WebSocket")]
    WebSocketConnectionError(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("Fuel error: {0}")]
    FuelError(#[from] fuels::types::errors::Error),

    #[error("Failed to retrieve environment variable {0}")]
    EnvVarError(#[from] VarError),

    #[error("Url parse error {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Rocket  error {0}")]
    RocketError(#[from] rocket::Error),

    #[error("Failed to parse from hex")]
    FromHexParseError(#[from] hex::FromHexError),

    #[error("Failed parse json serde")]
    FromJsonSerdeError(#[from] serde_json::Error),

    #[error("Fuel_crypto private key parsing error")]
    FuelCryptoPrivParseError,

    #[error("Failed to match orders: {0}")]
    MatchOrdersError(String),

    #[error("Failed to parse order amount: {0}")]
    OrderAmountParseError(String),

    #[error("Failed to parse contract ID")]
    ContractIdParseError(#[from] std::num::ParseIntError),

    #[error("Failed to parse contract ID")]
    ProcessMessagePayloadError(String),

    #[error("String parsing error: {0}")]
    StringParsingError(String),
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::StringParsingError(s.to_string())
    }
}

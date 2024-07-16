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

    #[error("Failed to parse from hex")]
    FromHexParseError(#[from] hex::FromHexError),

    #[error("Fuel_crypto private key parsing error")]
    FuelCryptoPrivParseError,
    

    #[error("Failed to parse contract ID")]
    ContractIdParseError(#[from] std::num::ParseIntError),
}

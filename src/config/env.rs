use anyhow::{Context, Result};
use std::env;

pub fn ev(key: &str) -> Result<String> {
    env::var(key).context(format!("Environment variable {} not found", key))
}

use anyhow::Result;

pub fn ev(key: &str) -> Result<String> {
    Ok(std::env::var(key)?)
}

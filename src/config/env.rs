use std::env;

use crate::error::Error;

pub fn ev(key: &str) -> Result<String, Error> {
    env::var(key).map_err(Error::EnvVarError)
}

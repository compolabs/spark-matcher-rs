use std::sync::Arc;

use rocket::{self, get, Route, State};
use rocket::serde::json::Json;
use rocket_okapi::{openapi, openapi_get_routes, JsonSchema};
use rocket_okapi::swagger_ui::SwaggerUIConfig;
use serde::Serialize;
use rocket_okapi::settings::UrlObject;
use tokio::sync::Mutex;

use crate::market::SparkMatcher;

#[derive(Serialize, JsonSchema)]
pub struct StatusMessage {
    pub message: String,
}


pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![
    ]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
        url: "/openapi.json".to_string(), 
        urls: vec![UrlObject::new("General", "/openapi.json")],
        ..Default::default()
    }
}


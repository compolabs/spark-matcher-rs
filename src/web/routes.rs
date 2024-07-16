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


#[openapi]
#[get("/start")]
pub async fn start_matcher(matcher: &State<Arc<Mutex<SparkMatcher>>>) -> Json<StatusMessage> {
    println!("Attempting to access matcher state");
    let matcher = matcher.lock().await;
    println!("Matcher state accessed, activating...");
    matcher.state.lock().await.activate();
    Json(StatusMessage { message: "Matcher activated.".to_string() })
}

#[openapi]
#[get("/stop")]
pub async fn stop_matcher(matcher: &State<Arc<Mutex<SparkMatcher>>>) -> Json<StatusMessage> {
    println!("Attempting to access matcher state");
    let matcher = matcher.lock().await;
    println!("Matcher state accessed, deactivating...");
    matcher.state.lock().await.deactivate();
    Json(StatusMessage { message: "Matcher deactivated.".to_string() })
}

#[openapi]
#[get("/status")]
pub async fn status_matcher(matcher: &State<Arc<Mutex<SparkMatcher>>>) -> Json<StatusMessage> {
    let matcher = matcher.lock().await;
    let status = format!("Matcher active: {:?}", matcher.state.lock().await.is_active());
    Json(StatusMessage { message: status})
}

#[openapi]
#[get("/debug")]
pub async fn debug_matcher_state(matcher: &State<Arc<Mutex<SparkMatcher>>>) -> String {
    let matcher = matcher.inner().clone();
    let matcher_locked = matcher.lock().await;
    let is_active = matcher_locked.state.lock().await.is_active();
    format!("Matcher is active: {}", is_active)
}


pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![
        start_matcher, 
        stop_matcher,
        status_matcher,
        debug_matcher_state
    ]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
        url: "/openapi.json".to_string(), 
        urls: vec![UrlObject::new("General", "/openapi.json")],
        ..Default::default()
    }
}


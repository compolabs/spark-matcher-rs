use std::sync::Arc;

use rocket::{self, get, Route, State};
use rocket::serde::json::Json;
use rocket_okapi::{openapi, openapi_get_routes, JsonSchema};
use rocket_okapi::swagger_ui::SwaggerUIConfig;
use serde::Serialize;
use rocket_okapi::settings::UrlObject;
use tokio::sync::{Mutex, RwLock};

use crate::market::matcher::MatcherCommand;
use crate::market::SparkMatcher;

#[derive(Serialize, JsonSchema)]
pub struct StatusMessage {
    pub message: String,
}


#[openapi]
#[get("/start")]
pub async fn start_matcher(matcher: &State<Arc<SparkMatcher>>) -> Json<StatusMessage> {
    matcher.command_sender.send(MatcherCommand::Start).await.unwrap();
    Json(StatusMessage { message: "Matcher activated.".to_string() })
}

#[openapi]
#[get("/stop")]
pub async fn stop_matcher(matcher: &State<Arc<SparkMatcher>>) -> Json<StatusMessage> {
    matcher.command_sender.send(MatcherCommand::Stop).await.unwrap();
    Json(StatusMessage { message: "Matcher deactivated.".to_string() })
}

#[openapi]
#[get("/status")]
pub async fn status_matcher(matcher: &State<Arc<SparkMatcher>>) -> Json<StatusMessage> {
    matcher.command_sender.send(MatcherCommand::Status).await.unwrap();
    Json(StatusMessage { message: "Status command sent.".to_string() })
}

pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![
        start_matcher, 
        stop_matcher,
        status_matcher,
    ]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
        url: "/openapi.json".to_string(), 
        urls: vec![UrlObject::new("General", "/openapi.json")],
        ..Default::default()
    }
}


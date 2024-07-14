use rocket::{self, get, Route};
use rocket::serde::json::Json;
use rocket_okapi::{openapi, openapi_get_routes, JsonSchema};
use rocket_okapi::swagger_ui::{make_swagger_ui, SwaggerUIConfig};
use serde::Serialize;

#[derive(Serialize, JsonSchema)]
pub struct StatusMessage {
    pub message: String,
}

#[openapi]
#[get("/start")]
pub fn start_matcher() -> Json<StatusMessage> {
    Json(StatusMessage { message: "Matcher started".to_string() })
}

#[openapi]
#[get("/stop")]
pub fn stop_matcher() -> Json<StatusMessage> {
    Json(StatusMessage { message: "Matcher stopped".to_string() })
}

pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![start_matcher, stop_matcher]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
    url: "/my_resource/openapi.json".to_string(),
    ..Default::default()
    }
}

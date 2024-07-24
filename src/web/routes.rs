use std::sync::Arc;

use rocket::{self, get, Route, State};
use rocket::serde::json::Json;
use rocket_okapi::{openapi, openapi_get_routes, JsonSchema};
use rocket_okapi::swagger_ui::SwaggerUIConfig;
use serde::Serialize;
use rocket_okapi::settings::UrlObject;
use sqlx::types::BigDecimal;
use sqlx::PgPool;

#[derive(Serialize, JsonSchema)]
pub struct StatsResponse {
    pub total_transactions: i64,
    pub avg_gas_used: String,
    pub total_gas_used: i64,
    pub avg_match_time_ms: String,
    pub buy_orders: i64,
    pub sell_orders: i64,
}

#[openapi]
#[get("/stats")]
async fn get_stats(db: &State<PgPool>) -> Json<StatsResponse> {
    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) AS total_transactions,
            COALESCE(AVG(avg_gas_used), 0) AS avg_gas_used,
            COALESCE(SUM(total_gas_used), 0) AS total_gas_used,
            COALESCE(AVG(match_time_ms), 0) AS avg_match_time_ms,
            COALESCE(SUM(buy_orders), 0) AS buy_orders,
            COALESCE(SUM(sell_orders), 0) AS sell_orders
        FROM transaction_stats
        "#,
    )
    .fetch_one(&**db)
    .await
    .unwrap();

    Json(StatsResponse {
        total_transactions: row.total_transactions.unwrap_or(0),
        avg_gas_used: row.avg_gas_used.unwrap_or(BigDecimal::from(0)).to_string(),
        total_gas_used: row.total_gas_used.unwrap_or(0),
        avg_match_time_ms: row.avg_match_time_ms.unwrap_or(BigDecimal::from(0)).to_string(),
        buy_orders: row.buy_orders.unwrap_or(0),
        sell_orders: row.sell_orders.unwrap_or(0),
    })
}

pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![
        get_stats,
    ]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
        url: "/openapi.json".to_string(), 
        urls: vec![UrlObject::new("General", "/openapi.json")],
        ..Default::default()
    }
}

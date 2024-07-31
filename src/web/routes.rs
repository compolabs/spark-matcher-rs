use std::sync::Arc;

use rocket::{self, get, Route, State};
use rocket::serde::json::Json;
use rocket_okapi::{openapi, openapi_get_routes, JsonSchema};
use rocket_okapi::swagger_ui::SwaggerUIConfig;
use serde::Serialize;
use rocket_okapi::settings::UrlObject;
use sqlx::types::BigDecimal;
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::management::manager::OrderManager;
use crate::model::SpotOrder;

#[derive(Serialize, JsonSchema)]
pub struct StatsResponse {
    pub total_transactions: i64,
    pub avg_gas_used: String,
    pub total_gas_used: i64,
    pub avg_match_time_ms: String,
    pub buy_orders: i64,
    pub sell_orders: i64,
    pub avg_receive_time_ms: String,
    pub avg_post_time_ms: String,
}

#[derive(Serialize, JsonSchema)]
pub struct OrdersResponse {
    pub orders: Vec<SpotOrder>,
}

#[derive(Serialize, JsonSchema)]
pub struct CurrentOrdersResponse {
    pub buy_orders: Vec<SpotOrder>,
    pub sell_orders: Vec<SpotOrder>,
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
            COALESCE(SUM(sell_orders), 0) AS sell_orders,
            COALESCE(AVG(receive_time_ms), 0) AS avg_receive_time_ms,
            COALESCE(AVG(post_time_ms), 0) AS avg_post_time_ms
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
        avg_receive_time_ms: row.avg_receive_time_ms.unwrap_or(BigDecimal::from(0)).to_string(),
        avg_post_time_ms: row.avg_post_time_ms.unwrap_or(BigDecimal::from(0)).to_string(),
    })
}

#[openapi]
#[get("/orders/buy")]
async fn get_buy_orders(manager: &State<Arc<OrderManager>>) -> Json<OrdersResponse> {
    let buy_orders = manager.get_all_buy_orders().await;
    Json(OrdersResponse { orders: buy_orders })
}

#[openapi]
#[get("/orders/sell")]
async fn get_sell_orders(manager: &State<Arc<OrderManager>>) -> Json<OrdersResponse> {
    let sell_orders = manager.get_all_sell_orders().await;
    Json(OrdersResponse { orders: sell_orders })
}

#[openapi]
#[get("/orders/all")]
async fn get_all_orders(manager: &State<Arc<OrderManager>>) -> Json<CurrentOrdersResponse> {
    let (buy_orders, sell_orders) = manager.get_all_orders().await;
    Json(CurrentOrdersResponse { buy_orders, sell_orders })
}

pub fn get_routes() -> Vec<Route> {
    openapi_get_routes![
        get_stats,
        get_buy_orders,
        get_sell_orders,
        get_all_orders,
    ]
}

pub fn get_docs() -> SwaggerUIConfig {
    SwaggerUIConfig {
        url: "/openapi.json".to_string(), 
        urls: vec![UrlObject::new("General", "/openapi.json")],
        ..Default::default()
    }
}

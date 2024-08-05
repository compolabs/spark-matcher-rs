use std::sync::Arc;

use rocket::{Build, Rocket};
use rocket_okapi::swagger_ui::make_swagger_ui;
use sqlx::PgPool;

use super::routes::{get_docs, get_routes};
use crate::management::manager::OrderManager;

pub fn rocket(db_pool: PgPool, order_manager: Arc<OrderManager>) -> Rocket<Build> {
    rocket::build()
        .manage(db_pool)
        .manage(order_manager)
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

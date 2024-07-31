use std::sync::Arc;

use rocket::{Rocket, Build};
use rocket_okapi::swagger_ui::make_swagger_ui;
use sqlx::PgPool;

use crate::management::manager::OrderManager;
use super::routes::{get_docs, get_routes};

pub fn rocket(db_pool: PgPool, order_manager: Arc<OrderManager>) -> Rocket<Build> {
    rocket::build()
        .manage(db_pool)
        .manage(order_manager)
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

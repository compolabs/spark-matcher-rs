use std::sync::Arc;

use rocket::{Rocket, Build};
use rocket_okapi::swagger_ui::make_swagger_ui;
use sqlx::PgPool;
use tokio::sync::Mutex;
use crate::market::SparkMatcher;

use super::routes::{get_docs, get_routes};

pub fn rocket(db_pool: PgPool) -> Rocket<Build> {
    rocket::build()
        .manage(db_pool)
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

use std::sync::Arc;

use rocket::{Rocket, Build};
use rocket_okapi::swagger_ui::make_swagger_ui;
use tokio::sync::Mutex;
use crate::market::SparkMatcher;

use super::routes::{get_docs, get_routes};

pub fn rocket() -> Rocket<Build> {
    rocket::build()
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

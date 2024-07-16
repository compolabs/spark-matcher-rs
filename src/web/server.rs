use std::sync::Arc;

use rocket::{Rocket, Build};
use rocket_okapi::swagger_ui::make_swagger_ui;
use tokio::sync::Mutex;
use crate::market::SparkMatcher;

use super::routes::{get_docs, get_routes};

pub fn rocket(matcher: Arc<Mutex<SparkMatcher>>) -> Rocket<Build> {
    rocket::build()
        .manage(matcher)
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

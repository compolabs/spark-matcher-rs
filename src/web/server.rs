use std::sync::Arc;

use rocket::{Rocket, Build, futures};
use rocket_okapi::swagger_ui::make_swagger_ui;
use tokio::sync::Mutex;
use crate::market::SparkMatcher;

use super::routes::{get_docs, get_routes};

pub fn rocket(matcher: Arc<Mutex<SparkMatcher>>) -> Rocket<Build> {
     println!("Registering matcher in Rocket state");
        if let Ok(matcher_lock) = Arc::clone(&matcher).try_lock() {
            println!("Matcher is currently in an unlocked state and can be managed");
        } else {
            println!("Failed to acquire lock on matcher, it may be in use elsewhere");
        }
    rocket::build()
        .manage(matcher)
        .mount("/", get_routes())
        .mount("/swagger", make_swagger_ui(&get_docs()))
}

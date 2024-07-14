use rocket::launch;
use rocket_okapi::swagger_ui::make_swagger_ui;

use super::routes::{get_docs, get_routes};

#[launch]
pub fn rocket() -> _ {
    rocket::build().mount("/", get_routes())
    .mount("/swagger", make_swagger_ui(&get_docs()))
}

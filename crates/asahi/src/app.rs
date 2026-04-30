use rocket::{Build, Rocket};

use crate::{
    api::{health, issues},
    store::IssueStore,
};

pub fn rocket() -> Rocket<Build> {
    rocket::build()
        .manage(IssueStore::default())
        .mount("/api", health::routes())
        .mount("/api", issues::routes())
}

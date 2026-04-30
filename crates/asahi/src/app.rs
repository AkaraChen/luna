use rocket::{Build, Rocket, fairing::AdHoc};

use crate::{
    api::{health, issues, notifications},
    db,
    service::IssueService,
    web,
};

pub fn rocket() -> Rocket<Build> {
    rocket_with_database_url(db::database_url_from_env())
}

pub fn rocket_with_database_url(database_url: impl Into<String>) -> Rocket<Build> {
    let database_url = database_url.into();
    rocket::build()
        .attach(AdHoc::try_on_ignite("Asahi Database", move |rocket| {
            Box::pin(async move {
                match db::connect_and_setup(&database_url).await {
                    Ok(db) => Ok(rocket.manage(IssueService::new(db))),
                    Err(err) => {
                        eprintln!("asahi database initialization failed: {err}");
                        Err(rocket)
                    }
                }
            })
        }))
        .mount("/api", health::routes())
        .mount("/api", issues::routes())
        .mount("/api", notifications::routes())
        .mount("/", web::routes())
}

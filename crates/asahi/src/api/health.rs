use rocket::{Route, get, routes};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

#[get("/healthz")]
fn healthz() -> rocket::serde::json::Json<HealthResponse> {
    rocket::serde::json::Json(HealthResponse { status: "ok" })
}

pub fn routes() -> Vec<Route> {
    routes![healthz]
}

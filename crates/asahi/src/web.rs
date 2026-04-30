use std::{io::Cursor, path::PathBuf};

use rocket::{
    Request, Route, get,
    http::ContentType,
    response::{self, Responder, Response},
    routes,
};

include!(concat!(env!("OUT_DIR"), "/embedded_web.rs"));

pub fn routes() -> Vec<Route> {
    routes![index, asset]
}

#[get("/")]
fn index() -> Option<EmbeddedAsset> {
    embedded_index()
        .or_else(fallback_index)
        .map(EmbeddedAsset::from)
}

#[get("/<path..>", rank = 20)]
fn asset(path: PathBuf) -> Option<EmbeddedAsset> {
    let path = path.to_string_lossy().replace('\\', "/");
    if path == "api" || path.starts_with("api/") {
        return None;
    }

    if let Some(asset) = embedded_asset(&path) {
        return Some(EmbeddedAsset::from(asset));
    }

    if path.contains('.') {
        return None;
    }

    embedded_index()
        .or_else(fallback_index)
        .map(EmbeddedAsset::from)
}

struct EmbeddedAsset {
    bytes: &'static [u8],
    content_type: &'static str,
}

impl From<(&'static [u8], &'static str)> for EmbeddedAsset {
    fn from((bytes, content_type): (&'static [u8], &'static str)) -> Self {
        Self {
            bytes,
            content_type,
        }
    }
}

impl<'r> Responder<'r, 'static> for EmbeddedAsset {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let content_type =
            ContentType::parse_flexible(self.content_type).unwrap_or(ContentType::Binary);

        Response::build()
            .header(content_type)
            .sized_body(self.bytes.len(), Cursor::new(self.bytes))
            .ok()
    }
}

fn fallback_index() -> Option<(&'static [u8], &'static str)> {
    Some((
        br#"<!doctype html><html><head><meta charset="utf-8"><title>Asahi</title></head><body><pre>Asahi dashboard is not embedded. Run `bun --cwd apps/asahi-web run build` before building crates/asahi.</pre></body></html>"#,
        "text/html; charset=utf-8",
    ))
}

#[cfg(test)]
mod tests {
    use rocket::{http::Status, local::blocking::Client};

    use crate::app;

    #[test]
    fn serves_index_at_root() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:")).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().is_some_and(|ct| ct.is_html()));
    }
}

//! Embedded Vue SPA (Phase 5).

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../ui-web/dist"]
struct Assets;

pub fn static_router() -> Router {
    Router::new()
        .route("/", get(static_handler))
        .route("/{*path}", get(static_handler))
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // Never shadow API / health (those are registered first on the outer router).
    if path.starts_with("v1/") || path == "health" {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess(path);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    // SPA fallback
    if let Some(index) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.to_vec()))
            .unwrap();
    }

    (
        StatusCode::NOT_FOUND,
        "UI not embedded — run: cd ui-web && npm ci && npm run build",
    )
        .into_response()
}

fn mime_guess(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json",
        "map" => "application/json",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

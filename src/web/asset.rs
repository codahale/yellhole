use std::path::Path;

use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get_service;
use axum::{http, Router};
use tokio::io;
use tower_http::services::ServeDir;

pub fn router(images_dir: &Path) -> Router {
    // TODO figure out how to handle 404 errors
    let images = get_service(ServeDir::new(images_dir)).handle_error(io_error);
    Router::new().nest("/images", images).route_layer(middleware::from_fn(cache_indefinitely))
}

async fn io_error(e: io::Error) -> StatusCode {
    log::warn!("error handling static asset: {}", e);
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn cache_indefinitely<B>(
    req: http::Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("max-age=31536000,immutable"),
    );
    Ok(resp)
}

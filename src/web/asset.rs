use std::path::Path;

use axum::http::StatusCode;
use axum::routing::get_service;
use axum::{http, Router};
use tokio::io;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn router(images_dir: &Path) -> Router {
    let images = get_service(
        ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("max-age=31536000,immutable"),
            ))
            .service(ServeDir::new(images_dir)),
    )
    .handle_error(io_error);
    Router::new().nest("/images", images)
}

async fn io_error(err: io::Error) -> StatusCode {
    tracing::warn!(%err, "error handling static asset");
    StatusCode::INTERNAL_SERVER_ERROR
}

use std::path::Path;

use axum::routing::get_service;
use axum::Router;
use tokio::io;
use tower_http::services::ServeDir;

use super::WebError;

pub fn router(images_dir: &Path) -> Router {
    // TODO figure out how to handle 404 errors
    let images = get_service(ServeDir::new(images_dir)).handle_error(io_error);
    Router::new().nest("/images", images)
}

async fn io_error(err: io::Error) -> WebError {
    WebError::IoError(err)
}

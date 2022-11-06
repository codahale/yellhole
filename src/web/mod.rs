use std::net::SocketAddr;

use askama::Template;
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

mod feed;

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
}

pub async fn serve(addr: &SocketAddr, db: SqlitePool) -> anyhow::Result<()> {
    let router = feed::router();

    let app = router
        .layer(AddExtensionLayer::new(Context { db }))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    log::info!("listening on http://{}", addr);
    axum::Server::bind(addr).serve(app.into_make_service()).await?;

    Ok(())
}

#[derive(Debug)]
pub struct Html<T: Template>(T);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(body) => axum::response::Html(body).into_response(),
            Err(_) => http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Debug, Error)]
pub enum WebError {
    #[error("entity not found")]
    NotFound,

    #[error("database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        match self {
            WebError::NotFound => (StatusCode::NOT_FOUND, Html(NotFoundPage)).into_response(),
            WebError::DatabaseError(e) => {
                log::error!("error querying database: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Html(InternalErrorPage)).into_response()
            }
        }
    }
}

// TODO expand to full template
#[derive(Template)]
#[template(source = "Not found.", ext = "html")]
struct NotFoundPage;

// TODO expand to full template
#[derive(Template)]
#[template(source = "Internal error.", ext = "html")]
struct InternalErrorPage;

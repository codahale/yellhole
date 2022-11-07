use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use askama::Template;
use axum::extract::multipart::MultipartError;
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::{io, signal};
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

mod admin;
mod feed;

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
    dir: PathBuf,
}

impl Context {
    pub fn images_dir(&self) -> PathBuf {
        let mut path = self.dir.clone();
        path.push("images");
        path
    }
}

pub async fn serve(addr: &SocketAddr, dir: impl AsRef<Path>, db: SqlitePool) -> anyhow::Result<()> {
    let router = feed::router().merge(admin::router());

    let app = router
        .layer(AddExtensionLayer::new(Context { db, dir: dir.as_ref().to_path_buf() }))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    log::info!("listening on http://{}", addr);
    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

#[derive(Debug)]
pub enum CacheControl {
    Immutable,
    MaxAge(Duration),
    NoCache,
}

impl fmt::Display for CacheControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheControl::Immutable => write!(f, "max-age=31536000,immutable"),
            CacheControl::MaxAge(d) => write!(f, "max-age={},must-revalidate", d.as_secs()),
            CacheControl::NoCache => write!(f, "no-cache"),
        }
    }
}

#[derive(Debug)]
pub struct Html<T: Template>(T, CacheControl);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(body) => (
                [
                    (
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
                    ),
                    (
                        http::header::CACHE_CONTROL,
                        http::HeaderValue::from_str(&self.1.to_string())
                            .expect("invalid Cache-Control value"),
                    ),
                ],
                body,
            )
                .into_response(),
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

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Multipart error: {0}")]
    MultipartError(#[from] MultipartError),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        match self {
            WebError::NotFound => {
                (StatusCode::NOT_FOUND, Html(NotFoundPage, CacheControl::NoCache)).into_response()
            }
            WebError::DatabaseError(e) => {
                log::error!("error querying database: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Html(InternalErrorPage, CacheControl::NoCache))
                    .into_response()
            }
            WebError::IoError(e) => {
                log::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Html(InternalErrorPage, CacheControl::NoCache))
                    .into_response()
            }
            WebError::MultipartError(e) => {
                log::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Html(InternalErrorPage, CacheControl::NoCache))
                    .into_response()
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

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}

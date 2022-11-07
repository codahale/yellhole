use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use askama::Template;
use axum::http::{self, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use futures::Future;
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

mod admin;
mod asset;
mod feed;

pub async fn serve(
    addr: &SocketAddr,
    dir: impl AsRef<Path>,
    db: SqlitePool,
    shutdown_hook: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let ctx = Context { db, dir: dir.as_ref().to_path_buf() };
    let app = feed::router()
        .merge(admin::router())
        .merge(asset::router(&ctx.images_dir()))
        .layer(AddExtensionLayer::new(ctx))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .route_layer(middleware::from_fn(handle_errors));

    log::info!("listening on http://{}", addr);
    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_hook)
        .await?;

    Ok(())
}

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

    pub fn uploads_dir(&self) -> PathBuf {
        let mut path = self.dir.clone();
        path.push("uploads");
        path
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
struct ErrorPage {
    uri: Uri,
    status: StatusCode,
}

async fn handle_errors<B>(req: http::Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let uri = req.uri().clone();
    let resp = next.run(req).await;
    if resp.status().is_client_error() || resp.status().is_server_error() {
        let page = ErrorPage { uri, status: resp.status() };
        if let Ok(body) = page.render() {
            return Ok((
                [(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
                )],
                body,
            )
                .into_response());
        }
        dbg!(&resp.status());
    }
    Ok(resp)
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

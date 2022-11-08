use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use askama::Template;
use axum::http::{self, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use futures::Future;
use sqlx::SqlitePool;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::TraceLayer;
use url::Url;

mod admin;
mod asset;
mod feed;

pub async fn serve(
    addr: &SocketAddr,
    ctx: Context,
    shutdown_hook: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let base_url = ctx.base_url.clone();
    let app = feed::router()
        .merge(admin::router())
        .merge(asset::router(&ctx.images_dir))
        .layer(AddExtensionLayer::new(ctx))
        .route_layer(middleware::from_fn(handle_errors))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(http::header::COOKIE)))
        .layer(TraceLayer::new_for_http())
        .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(http::header::SET_COOKIE)))
        .layer(PropagateRequestIdLayer::x_request_id());

    tracing::info!(%addr, %base_url, "starting server");
    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_hook)
        .await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
    base_url: Url,
    name: String,
    author: String,
    images_dir: PathBuf,
    uploads_dir: PathBuf,
}

impl Context {
    pub fn new(
        db: SqlitePool,
        base_url: Url,
        name: String,
        author: String,
        images_dir: PathBuf,
        uploads_dir: PathBuf,
    ) -> Context {
        Context { db, base_url, name, author, images_dir, uploads_dir }
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

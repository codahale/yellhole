use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use askama::Template;
use axum::http::{self, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum_sessions::{SameSite, SessionLayer};
use futures::Future;
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::trace::TraceLayer;
use url::Url;
use webauthn_rs::{Webauthn, WebauthnBuilder};

use crate::models::DbSessionStore;

mod admin;
mod asset;
mod auth;
mod feed;

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
    base_url: Url,
    name: String,
    author: String,
    images_dir: PathBuf,
    uploads_dir: PathBuf,
    webauthn: Arc<Webauthn>,
}

impl Context {
    pub async fn new(
        db: SqlitePool,
        base_url: Url,
        name: String,
        author: String,
        data_dir: impl AsRef<Path>,
    ) -> Result<Context, anyhow::Error> {
        // Create the images and uploads directories, if necessary.
        let images_dir = data_dir.as_ref().join("images");
        tracing::info!(?images_dir, "creating directory");
        tokio::fs::create_dir_all(&images_dir).await?;

        let uploads_dir = data_dir.as_ref().join("uploads");
        tracing::info!(?uploads_dir, "creating directory");
        tokio::fs::create_dir_all(&uploads_dir).await?;

        // Create a WebAuthn context.
        let webauthn = WebauthnBuilder::new(
            &base_url
                .host()
                .ok_or_else(|| anyhow::anyhow!("base URL must include a host"))?
                .to_string(),
            &base_url,
        )?
        .build()?;

        Ok(Context {
            db,
            base_url,
            name,
            author,
            images_dir,
            uploads_dir,
            webauthn: Arc::new(webauthn),
        })
    }

    pub async fn serve(
        self,
        addr: &SocketAddr,
        shutdown_hook: impl Future<Output = ()>,
    ) -> anyhow::Result<()> {
        tracing::info!(%addr, base_url=%self.base_url, "starting server");

        // Store sessions in the database. Use a constant key here because the cookie value is just
        // a random ID.
        let store = DbSessionStore::new(&self.db);
        let session_layer = SessionLayer::new(store, &[69; 64])
            .with_cookie_name("yellhole")
            .with_same_site_policy(SameSite::Strict)
            .with_secure(self.base_url.scheme() == "https");

        let app = admin::router()
            .merge(auth::router())
            .layer(session_layer) // only enable sessions for auth and admin
            .merge(feed::router())
            .merge(asset::router(&self.images_dir))
            .layer(
                ServiceBuilder::new()
                    .layer(AddExtensionLayer::new(self))
                    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                    .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
                        http::header::COOKIE,
                    )))
                    .layer(TraceLayer::new_for_http())
                    .layer(SetSensitiveResponseHeadersLayer::new(std::iter::once(
                        http::header::SET_COOKIE,
                    )))
                    .layer(PropagateRequestIdLayer::x_request_id())
                    .layer(middleware::from_fn(handle_errors)),
            );

        axum::Server::bind(addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(shutdown_hook)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
struct ErrorPage {
    status: StatusCode,
}

impl ErrorPage {
    fn for_status(status: StatusCode) -> Response {
        let mut resp = Page(ErrorPage { status }).into_response();
        *resp.status_mut() = status;
        resp
    }
}

async fn handle_errors<B>(req: http::Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let resp = next.run(req).await;
    if (resp.status().is_client_error() || resp.status().is_server_error())
        && resp.status() != StatusCode::UNAUTHORIZED
    {
        return Ok(ErrorPage::for_status(resp.status()));
    }
    Ok(resp)
}

#[derive(Debug)]
pub struct Page<T: Template>(T);

impl<T: Template> IntoResponse for Page<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(body) => Html(body).into_response(),
            Err(err) => {
                tracing::error!(?err, "unable to render template");
                http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

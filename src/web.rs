use std::net::SocketAddr;
use std::sync::Arc;

use askama::Template;
use axum::http::{self, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum_sessions::{SameSite, SessionLayer};
use futures::Future;
use sqlx::SqlitePool;
use tokio::task;
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::trace::TraceLayer;
use url::Url;
use webauthn_rs::WebauthnBuilder;

use crate::config::{Author, DataDir, Title};
use crate::models::DbSessionStore;

mod admin;
mod asset;
mod auth;
mod feed;

#[derive(Debug)]
pub struct App {
    db: SqlitePool,
    data_dir: DataDir,
    base_url: Url,
    title: Title,
    author: Author,
}

impl App {
    pub fn new(
        db: SqlitePool,
        data_dir: DataDir,
        base_url: Url,
        title: Title,
        author: Author,
    ) -> App {
        App { db, data_dir, base_url, title, author }
    }

    pub async fn serve(
        self,
        addr: &SocketAddr,
        shutdown_hook: impl Future<Output = ()>,
    ) -> anyhow::Result<()> {
        tracing::info!(%addr, base_url=%self.base_url, "starting server");

        // Create a WebAuthn context.
        let webauthn =
            WebauthnBuilder::new(self.base_url.host_str().unwrap(), &self.base_url)?.build()?;

        // Store sessions in the database. Use a constant key here because the cookie value is just
        // a random ID.
        let store = DbSessionStore::new(&self.db);
        let session_expiry = task::spawn(store.clone().continuously_delete_expired());
        let session_layer = SessionLayer::new(store, &[69; 64])
            .with_cookie_name("yellhole")
            .with_same_site_policy(SameSite::Strict)
            .with_secure(self.base_url.scheme() == "https");

        let app = admin::router()
            .merge(auth::router())
            .layer(session_layer) // only enable sessions for auth and admin
            .merge(feed::router())
            .merge(asset::router(self.data_dir.images_dir()))
            .layer(
                ServiceBuilder::new()
                    .layer(AddExtensionLayer::new(self.base_url))
                    .layer(AddExtensionLayer::new(self.db))
                    .layer(AddExtensionLayer::new(Arc::new(webauthn)))
                    .layer(AddExtensionLayer::new(self.author))
                    .layer(AddExtensionLayer::new(self.title))
                    .layer(AddExtensionLayer::new(self.data_dir))
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

        session_expiry.await??;

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

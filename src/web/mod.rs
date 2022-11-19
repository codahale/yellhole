use std::net::SocketAddr;
use std::path::PathBuf;

use askama::Template;
use axum::http::{self, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use futures::Future;
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::request_id::MakeRequestUuid;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::ServiceBuilderExt;
use tracing::Level;
use url::Url;

use crate::config::{Author, Title};
use crate::services::images::ImageService;
use crate::services::notes::NoteService;
use crate::services::passkeys::PasskeyService;
use crate::services::sessions::SessionService;

mod admin;
mod asset;
mod auth;
mod feed;

#[derive(Debug)]
pub struct App {
    db: SqlitePool,
    data_dir: PathBuf,
    base_url: Url,
    title: Title,
    author: Author,
}

impl App {
    pub fn new(
        db: SqlitePool,
        data_dir: PathBuf,
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

        let (sessions, session_expiry) = SessionService::new(&self.db, &self.base_url);
        let images = ImageService::new(self.db.clone(), &self.data_dir)?;

        let app = admin::router()
            .route_layer(middleware::from_extractor::<auth::RequireAuth>())
            .merge(auth::router())
            .layer(sessions) // only enable sessions for auth and admin
            .merge(feed::router())
            .merge(asset::router(self.data_dir.join("images")))
            .layer(
                ServiceBuilder::new()
                    .add_extension(PasskeyService::new(self.db.clone(), &self.base_url))
                    .add_extension(images)
                    .add_extension(NoteService::new(self.db.clone()))
                    .add_extension(self.base_url)
                    .add_extension(self.author)
                    .add_extension(self.title)
                    .set_x_request_id(MakeRequestUuid)
                    .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
                        http::header::COOKIE,
                    )))
                    .layer(
                        TraceLayer::new_for_http()
                            .make_span_with(
                                DefaultMakeSpan::new().level(Level::INFO).include_headers(true),
                            )
                            .on_response(
                                DefaultOnResponse::new().level(Level::INFO).include_headers(true),
                            ),
                    )
                    .layer(SetSensitiveResponseHeadersLayer::new(std::iter::once(
                        http::header::SET_COOKIE,
                    )))
                    .propagate_x_request_id()
                    .layer(middleware::from_fn(handle_errors))
                    .catch_panic(),
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
    if resp.status().is_server_error() || resp.status() == StatusCode::NOT_FOUND {
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

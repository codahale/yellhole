use std::fs;
use std::path::PathBuf;

use askama::Template;
use axum::http::{self, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::request_id::MakeRequestUuid;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::ServiceBuilderExt;
use tracing::Level;

use crate::config::Config;
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
    config: Config,
}

impl App {
    pub async fn new(config: Config) -> Result<App, anyhow::Error> {
        anyhow::ensure!(config.base_url.path() == "/", "base URL must not have a path");
        anyhow::ensure!(config.base_url.host().is_some(), "base URL must have a host");

        // Initialize the data directory.
        let data_dir = config.data_dir.canonicalize()?;
        fs::create_dir_all(&data_dir)?;

        // Connect to the DB.
        let db_path = data_dir.join("yellhole.db");
        tracing::info!(?db_path, "opening database");
        let db_opts = SqliteConnectOptions::new().create_if_missing(true).filename(db_path);
        let db = SqlitePoolOptions::new().connect_with(db_opts).await?;

        // Run any pending migrations.
        tracing::info!("running migrations");
        sqlx::migrate!().run(&db).await?;

        Ok(App { db, data_dir, config })
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let addr = &([0, 0, 0, 0], self.config.port).into();
        tracing::info!(%addr, base_url=%self.config.base_url, "starting server");

        let (sessions, session_expiry) = SessionService::new(&self.db, &self.config.base_url);
        let images = ImageService::new(self.db.clone(), &self.data_dir)?;

        let app = admin::router()
            .route_layer(middleware::from_extractor::<auth::RequireAuth>())
            .merge(auth::router())
            .layer(sessions) // only enable sessions for auth and admin
            .merge(feed::router())
            .merge(asset::router(images.images_dir()))
            .layer(
                ServiceBuilder::new()
                    .add_extension(PasskeyService::new(self.db.clone(), &self.config.base_url))
                    .add_extension(images)
                    .add_extension(NoteService::new(self.db.clone()))
                    .add_extension(self.config)
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
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        session_expiry.await??;

        Ok(())
    }
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

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = signal::ctrl_c().await {
            tracing::error!(%err, "unable to install ^C signal handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut h) => {
                h.recv().await;
            }
            Err(err) => {
                tracing::error!(%err, "unable to install SIGTERM handler");
            }
        };
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("starting graceful shutdown");
}

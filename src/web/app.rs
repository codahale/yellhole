use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{fs, io};

use askama::Template;
use axum::http::{self, StatusCode, Uri};
use axum::middleware::{self};
use axum::response::{Html, IntoResponse, Response};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::task;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::request_id::MakeRequestUuid;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::trace::TraceLayer;
use tower_http::ServiceBuilderExt;

use crate::config::Config;
use crate::services::assets::AssetService;
use crate::services::images::ImageService;
use crate::services::notes::NoteService;
use crate::services::passkeys::PasskeyService;
use crate::services::sessions::SessionService;
use crate::web::{admin, asset, auth, feed};

/// The Yellhole application.
#[derive(Debug)]
pub struct App {
    db: SqlitePool,
    config: Config,
}

impl App {
    /// Create a new [`App`] from the given config.
    pub async fn new(mut config: Config) -> Result<App, anyhow::Error> {
        anyhow::ensure!(config.base_url.path() == "/", "base URL must not have a path");
        anyhow::ensure!(config.base_url.host().is_some(), "base URL must have a host");

        // Initialize the data directory.
        config.data_dir = config.data_dir.canonicalize()?;
        fs::create_dir_all(&config.data_dir)?;

        // Connect to the DB.
        let db_path = config.data_dir.join("yellhole.db");
        tracing::info!(?db_path, "opening database");
        let db_opts = SqliteConnectOptions::new().create_if_missing(true).filename(db_path);
        let db = SqlitePoolOptions::new().connect_with(db_opts).await?;

        // Run any pending migrations.
        tracing::info!("running migrations");
        sqlx::migrate!().run(&db).await?;

        Ok(App { db, config })
    }

    /// Listen for HTTP requests.
    pub async fn serve(self) -> anyhow::Result<()> {
        let addr = SocketAddr::new(self.config.addr, self.config.port);
        tracing::info!(%addr, base_url=%self.config.base_url, "starting server");

        // Create a new application state.
        let state = AppState::new(self.db, self.config)?;

        // Spawn a background task for deleting expired sessions.
        let expiry = task::spawn(state.sessions.clone().continuously_delete_expired());

        // Create a full stack of routers, state, and middleware.
        let app = admin::router()
            .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
            .merge(auth::router())
            .merge(feed::router())
            .merge(asset::router(&state.images, &state.assets)?)
            .with_state(state)
            .fallback(not_found)
            .layer(
                ServiceBuilder::new()
                    .set_x_request_id(MakeRequestUuid)
                    .layer(SetSensitiveRequestHeadersLayer::new([http::header::COOKIE]))
                    .layer(TraceLayer::new_for_http())
                    .layer(SetSensitiveResponseHeadersLayer::new([http::header::SET_COOKIE]))
                    .propagate_x_request_id()
                    .layer(CatchPanicLayer::custom(handle_panic)),
            );

        // Listen for requests, handling a graceful shutdown.
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(elegant_departure::tokio::depart().on_termination())
            .await?;

        // Wait for background task to exit.
        expiry.await??;

        Ok(())
    }
}

/// The shared state of a running Yellhole instance.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub assets: AssetService,
    pub images: ImageService,
    pub notes: NoteService,
    pub passkeys: PasskeyService,
    pub sessions: SessionService,
}

impl AppState {
    /// The timestamp of the build, injected by vergen via build.rs.
    pub const BUILD_TIMESTAMP: &'static str = env!("VERGEN_BUILD_TIMESTAMP");

    /// Create a new [`AppState`] with the given database and config.
    pub fn new(db: SqlitePool, config: Config) -> Result<AppState, io::Error> {
        let images = ImageService::new(db.clone(), &config.data_dir)?;
        let passkeys = PasskeyService::new(db.clone(), config.base_url.clone());
        Ok(AppState {
            config: Arc::new(config),
            assets: AssetService::new()?,
            images,
            notes: NoteService::new(db.clone()),
            passkeys,
            sessions: SessionService::new(db),
        })
    }
}

/// A common error type for application errors which map to responses.
#[derive(Debug, Error)]
pub enum AppError {
    /// The most generic error type. Returns a 500.
    #[error(transparent)]
    Generic(#[from] anyhow::Error),

    /// Any failure of an interaction with the database. Returns a 500.
    #[error(transparent)]
    QueryFailure(#[from] sqlx::Error),

    /// When a page doesn't exist. Returns a 404.
    #[error("resource not found")]
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Generic(_) | AppError::QueryFailure(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound => StatusCode::NOT_FOUND,
        };
        ErrorPage::for_status(status).into_response()
    }
}

#[derive(Debug)]
pub struct Page<T: Template>(pub T);

impl<T: Template> IntoResponse for Page<T> {
    fn into_response(self) -> Response {
        let html = self.0.render().expect("error rendering template");
        let cfg = minify_html_onepass::Cfg { minify_js: true, minify_css: true };
        let html = minify_html_onepass::copy(html.as_bytes(), &cfg).expect("error minifying HTML");
        Html(html).into_response()
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    status: StatusCode,
}

impl ErrorPage {
    pub fn for_status(status: StatusCode) -> Response {
        (status, Page(ErrorPage { status })).into_response()
    }
}

/// Given a recovered panic value from a handler, log it as an error and return a 500.
fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response {
    let details = if let Some(s) = err.downcast_ref::<String>() {
        s.as_str()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        s
    } else {
        "Unknown panic message"
    };
    tracing::error!(err = details, "panic in handler");
    ErrorPage::for_status(StatusCode::INTERNAL_SERVER_ERROR).into_response()
}

/// Map all unknown URIs to 404 errors.
#[tracing::instrument(err)]
async fn not_found(_uri: Uri) -> Result<(), AppError> {
    Err(AppError::NotFound)
}

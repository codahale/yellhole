use std::any::Any;
use std::{fs, io};

use axum::http::{self, StatusCode, Uri};
use axum::middleware::{self};
use axum::response::{IntoResponse, Response};
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
use url::Url;

use crate::config::Config;
use crate::services::images::ImageService;
use crate::services::notes::NoteService;
use crate::services::passkeys::PasskeyService;
use crate::services::sessions::SessionService;
use crate::web::{admin, asset, auth, feed};

use super::pages::ErrorPage;
#[derive(Debug)]
pub struct App {
    db: SqlitePool,
    config: Config,
}

impl App {
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

    pub async fn serve(self) -> anyhow::Result<()> {
        let addr = &([0, 0, 0, 0], self.config.port).into();
        tracing::info!(%addr, base_url=%self.config.base_url, "starting server");

        let state = AppState::new(self.db, self.config)?;
        let expiry = task::spawn(state.sessions.clone().continuously_delete_expired());
        let app = admin::router()
            .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth))
            .merge(auth::router())
            .merge(feed::router())
            .merge(asset::router(state.images.images_dir()))
            .fallback(not_found)
            .layer(
                ServiceBuilder::new()
                    .set_x_request_id(MakeRequestUuid)
                    .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
                        http::header::COOKIE,
                    )))
                    .layer(TraceLayer::new_for_http())
                    .layer(SetSensitiveResponseHeadersLayer::new(std::iter::once(
                        http::header::SET_COOKIE,
                    )))
                    .propagate_x_request_id()
                    .layer(CatchPanicLayer::custom(handle_panic)),
            )
            .with_state(state);

        axum::Server::bind(addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(elegant_departure::tokio::depart().on_termination())
            .await?;

        expiry.await??;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub author: String,
    pub title: String,
    pub notes: NoteService,
    pub passkeys: PasskeyService,
    pub base_url: Url,
    pub sessions: SessionService,
    pub images: ImageService,
}

impl AppState {
    pub fn new(db: SqlitePool, config: Config) -> Result<AppState, io::Error> {
        let author = config.author;
        let title = config.title;
        let notes = NoteService::new(db.clone());
        let passkeys = PasskeyService::new(db.clone(), config.base_url.clone());
        let base_url = config.base_url;
        let sessions = SessionService::new(db.clone());
        let images = ImageService::new(db, &config.data_dir)?;
        Ok(AppState { author, title, notes, passkeys, base_url, sessions, images })
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Generic(#[from] anyhow::Error),

    #[error(transparent)]
    QueryFailure(#[from] sqlx::Error),

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

#[tracing::instrument(err)]
async fn not_found(uri: Uri) -> Result<(), AppError> {
    Err(AppError::NotFound)
}

#![cfg(test)]

use std::{ffi::OsString, io, net::SocketAddr};

use axum::Router;
use clap::Parser;
use include_dir::{include_dir, Dir};
use reqwest::{redirect::Policy, Client, ClientBuilder, RequestBuilder, Url};
use rusqlite_migration::AsyncMigrations;
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_rusqlite::Connection;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::{
    fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use crate::{config::Config, web::AppState};

pub struct TestEnv {
    pub db: Connection,
    pub state: AppState,
    pub temp_dir: TempDir,
}

impl TestEnv {
    pub async fn new() -> Result<TestEnv, anyhow::Error> {
        // TODO add fixture support
        let temp_dir = TempDir::new()?;
        let mut config =
            Config::try_parse_from::<_, OsString>([]).expect("should parse empty command line");
        config.data_dir = temp_dir.path().to_path_buf();
        config.base_url = "http://example.com".parse().expect("should be a valid URL");
        let mut db = Connection::open_in_memory().await?;
        let migrations = AsyncMigrations::from_directory(&MIGRATIONS_DIR)?;
        migrations.to_latest(&mut db).await?;
        let state = AppState::new(db.clone(), config)?;
        Ok(TestEnv { db, state, temp_dir })
    }

    pub async fn into_server(self, app: Router<AppState>) -> Result<TestServer, anyhow::Error> {
        let _ = tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off")))
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::FULL).pretty())
            .try_init();

        let listener = TcpListener::bind::<SocketAddr>(([127, 0, 0, 1], 0).into()).await?;
        let addr = listener.local_addr()?;
        let app = app.layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO).include_headers(true))
                .on_response(DefaultOnResponse::new().level(Level::INFO).include_headers(true)),
        );
        let server = TestServer {
            db: self.db,
            url: Url::parse(&format!("http://{addr}/"))?,
            client: ClientBuilder::new().redirect(Policy::none()).cookie_store(true).build()?,
            _temp_dir: self.temp_dir,
            state: self.state.clone(),
            handle: tokio::spawn(async move {
                axum::serve(listener, app.with_state(self.state).into_make_service()).await
            }),
        };

        Ok(server)
    }
}

static MIGRATIONS_DIR: Dir = include_dir!("migrations");

pub struct TestServer {
    pub db: Connection,
    pub url: Url,
    client: Client,
    _temp_dir: TempDir,
    pub state: AppState,
    handle: JoinHandle<io::Result<()>>,
}

impl TestServer {
    pub fn get(&self, path: &str) -> RequestBuilder {
        self.client.get(self.url.join(path).expect("should be a valid URL"))
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.client.post(self.url.join(path).expect("should be a valid URL"))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

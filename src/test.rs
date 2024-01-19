#![cfg(test)]

use std::net::SocketAddr;

use axum::Router;
use clap::Parser;
use reqwest::{redirect::Policy, Client, ClientBuilder, RequestBuilder, Url};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::{
    fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use crate::{config::Config, web::AppState};

pub struct TestEnv {
    pub state: AppState,
    pub temp_dir: TempDir,
}

impl TestEnv {
    pub fn new(db: SqlitePool) -> Result<TestEnv, anyhow::Error> {
        let temp_dir = TempDir::new()?;
        let config = Config::parse_from([
            format!("--data-dir={}", temp_dir.path().to_str().unwrap()),
            "--base-url=http://example.com".into(),
        ]);
        let state = AppState::new(db, config)?;
        Ok(TestEnv { state, temp_dir })
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
            url: Url::parse(&format!("http://{addr}/"))?,
            client: ClientBuilder::new().redirect(Policy::none()).cookie_store(true).build()?,
            _temp_dir: self.temp_dir,
            state: self.state.clone(),
        };

        tokio::spawn(async move {
            axum::serve(listener, app.with_state(self.state).into_make_service()).await.unwrap()
        });

        Ok(server)
    }
}

pub struct TestServer {
    pub url: Url,
    client: Client,
    _temp_dir: TempDir,
    pub state: AppState,
}

impl TestServer {
    pub fn get(&self, path: &str) -> RequestBuilder {
        self.client.get(self.url.join(path).unwrap())
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.client.post(self.url.join(path).unwrap())
    }
}

#![cfg(test)]

use std::net::{SocketAddr, TcpListener};

use axum::Router;
use clap::Parser;
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, RequestBuilder, Url};
use sqlx::SqlitePool;
use tempdir::TempDir;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::web::AppState;

pub struct TestEnv {
    pub state: AppState,
    pub temp_dir: TempDir,
}

impl TestEnv {
    pub fn new(db: SqlitePool) -> Result<TestEnv, anyhow::Error> {
        let temp_dir = TempDir::new("yellhole-test")?;
        let config = Config::parse_from([
            format!("--data-dir={}", temp_dir.path().to_str().unwrap()),
            "--base-url=http://example.com".into(),
        ]);
        let state = AppState::new(db, config)?;
        Ok(TestEnv { state, temp_dir })
    }

    pub fn into_server(self, app: Router<AppState>) -> Result<TestServer, anyhow::Error> {
        let _ = tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off")))
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::FULL).pretty())
            .try_init();

        let listener = TcpListener::bind::<SocketAddr>(([127, 0, 0, 1], 0).into())?;
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
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.with_state(self.state).into_make_service())
                .await
                .unwrap();
        });

        Ok(server)
    }
}

pub struct TestServer {
    url: Url,
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

use std::fs;

use clap::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::signal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::Config;
use crate::web::App;

mod config;
mod services;
#[cfg(test)]
mod test_server;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse the command line args.
    let config = Config::parse();
    anyhow::ensure!(config.base_url.path() == "/", "base URL must not have a path");
    anyhow::ensure!(config.base_url.host().is_some(), "base URL must have a host");

    // Initialize the data directory.
    let data_dir = config.data_dir.canonicalize()?;
    fs::create_dir_all(&data_dir)?;

    // Configure tracing, defaulting to debug levels.
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "trace,yellhole=debug,sqlx=info,hyper=info,mio=info,tower_http=debug".into()
        })))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Connect to the DB.
    let db_path = data_dir.join("yellhole.db");
    tracing::info!(?db_path, "opening database");
    let db_opts = SqliteConnectOptions::new().create_if_missing(true).filename(db_path);
    let db = SqlitePoolOptions::new().connect_with(db_opts).await?;

    // Run any pending migrations.
    tracing::info!("running migrations");
    sqlx::migrate!().run(&db).await?;

    // Spin up an HTTP server and listen for requests.
    App::new(db, data_dir, config.base_url, config.title, config.author)
        .serve(&([0, 0, 0, 0], config.port).into(), shutdown_signal())
        .await
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

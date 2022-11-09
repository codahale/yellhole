use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::signal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

mod models;
mod web;

#[derive(Debug, Parser)]
struct Config {
    /// Listen for requests on the given address.
    #[clap(long, default_value = "127.0.0.1:3000", env("LISTEN_ADDR"))]
    listen_addr: SocketAddr,

    /// The directory in which all persistent data is stored.
    #[clap(long, default_value = "./data", env("DATA_DIR"))]
    data_dir: PathBuf,

    /// The time zone to be used for formatting and parsing dates and times.
    #[clap(long)]
    time_zone: Option<String>,

    /// The base URL for the web server.
    #[clap(long, default_value = "http://127.0.0.1:3000/", env("BASE_URL"))]
    base_url: Url,

    /// The name of the Yellhole instance.
    #[clap(long, default_value = "Yellhole", env("NAME"))]
    name: String,

    /// The name of the person posting this crap.
    #[clap(long, default_value = "Luther Blissett", env("AUTHOR"))]
    author: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse the command line args.
    let config = Config::parse();
    let dir = config.data_dir.canonicalize()?;

    // Override the TZ env var with any command line option for time zone.
    if let Some(tz) = config.time_zone {
        std::env::set_var("TZ", tz);
    }

    // Configure tracing, defaulting to debug levels.
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "yellhole=debug,sqlx=info,hyper=info,mio=info,tower_http=debug".into()
        })))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create the images and uploads directories, if necessary.
    let mut images_dir = dir.clone();
    images_dir.push("images");
    tracing::info!(?images_dir, "creating directory");
    tokio::fs::create_dir_all(&images_dir).await?;

    let mut uploads_dir = dir.clone();
    uploads_dir.push("uploads");
    tracing::info!(?uploads_dir, "creating directory");
    tokio::fs::create_dir_all(&uploads_dir).await?;

    // Connect to the DB.
    let mut db_path = dir.clone();
    tracing::info!(?db_path, "opening database");
    db_path.push("yellhole.db");
    let db_opts = SqliteConnectOptions::new().filename(db_path);
    let db = SqlitePoolOptions::new().connect_with(db_opts).await?;

    // Run any pending migrations.
    tracing::info!("running migrations");
    sqlx::migrate!().run(&db).await?;

    // Spin up an HTTP server and listen for requests.
    let ctx =
        web::Context::new(db, config.base_url, config.name, config.author, images_dir, uploads_dir);
    web::serve(&config.listen_addr, ctx, shutdown_signal()).await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}

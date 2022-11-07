use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse the command line args.
    let config = Config::parse();

    // Override the TZ env var with any command line option for time zone.
    if let Some(tz) = config.time_zone {
        std::env::set_var("TZ", tz);
    }

    // Use the debug level as a default and configure logging.
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug,hyper=info,mio=info");
    }
    tracing_subscriber::fmt::init();

    // Create the images directory, if necessary.
    let mut images_dir = config.data_dir.clone();
    images_dir.push("images");
    tokio::fs::create_dir_all(&images_dir).await?;

    // Connect to the DB.
    let mut db_path = config.data_dir.clone();
    log::info!("loading data from {:?}", &db_path);
    db_path.push("yellhole.db");
    let db_opts = SqliteConnectOptions::new().filename(db_path);
    let db = SqlitePoolOptions::new().connect_with(db_opts).await?;

    // Run any pending migrations.
    log::info!("checking for migrations");
    sqlx::migrate!().run(&db).await?;

    // Spin up an HTTP server and listen for requests.
    web::serve(&config.listen_addr, &config.data_dir, db).await
}

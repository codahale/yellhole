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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file, if any, and parse command line args.
    let _ = dotenvy::dotenv();
    let config = Config::parse();

    // Use the debug level as a default and configure logging.
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug,hyper=info,mio=info");
    }
    tracing_subscriber::fmt::init();

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
    web::serve(&config.listen_addr, db).await
}

use std::net::SocketAddr;
use std::str::FromStr;

use clap::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

mod web;

#[derive(Debug, Parser)]
struct Config {
    /// Listen for requests on the given address.
    #[clap(long, default_value = "127.0.0.1:3000", env("LISTEN_ADDR"))]
    listen_addr: SocketAddr,

    /// Connect the the given database.
    #[clap(long, env("DATABASE_URL"))]
    database_url: String,
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
    log::info!("connecting to {}", &config.database_url);
    let db = SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::from_str(&config.database_url)?
                .journal_mode(SqliteJournalMode::Wal)
                .create_if_missing(true),
        )
        .await?;

    // Run any pending migrations.
    log::info!("checking for migrations");
    sqlx::migrate!().run(&db).await?;

    // Spin up an HTTP server and listen for requests.
    web::serve(&config.listen_addr, db).await
}

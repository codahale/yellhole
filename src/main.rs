use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::web::App;

mod config;
mod services;
#[cfg(test)]
mod test_server;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure tracing, defaulting to INFO except for sqlx, which is wild chatty.
    tracing_subscriber::registry()
        .with(EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".into())))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    // Parse the command line args.
    let config = Config::parse();

    // Spin up an HTTP server and listen for requests.
    App::new(config).await?.serve().await
}

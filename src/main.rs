use clap::Parser;
use tikv_jemallocator::Jemalloc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{config::Config, web::App};

mod config;
mod services;
mod test;
mod web;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure tracing, defaulting to INFO except for sqlx, which is wild chatty, and tower_http,
    // which is too terse.
    tracing_subscriber::registry()
        .with(EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    // Parse the command line args.
    let config = Config::parse();

    // Spin up an HTTP server and listen for requests.
    App::new(config).await?.serve().await
}

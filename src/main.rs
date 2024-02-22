use std::env;

use clap::Parser;
use tikv_jemallocator::Jemalloc;

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
    env::set_var(
        "RUST_LOG",
        env::var("RUST_LOG").unwrap_or("info,sqlx=warn,tower_http=debug".into()),
    );

    // Enable tokio-console.
    console_subscriber::ConsoleLayer::builder().with_default_env().init();

    // Parse the command line args.
    let config = Config::parse();

    // Spin up an HTTP server and listen for requests.
    App::new(config).await?.serve().await
}

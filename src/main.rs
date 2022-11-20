use clap::Parser;
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
    // Configure tracing, defaulting to debug levels.
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "trace,yellhole=debug,sqlx=info,hyper=info,mio=info,tower_http=debug".into()
        })))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse the command line args.
    let config = Config::parse();

    // Spin up an HTTP server and listen for requests.
    App::new(config).await?.serve().await
}

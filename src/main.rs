use std::net::SocketAddr;

use askama::Template;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{http, Router};
use clap::Parser;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

#[derive(Debug, Parser)]
struct Cli {
    /// Listen for requests on the given address.
    #[clap(long, default_value = "127.0.0.1:3000")]
    addr: SocketAddr,

    /// Log entries with the given level or higher.
    #[clap(short = 'l', long = "log", default_value = "debug")]
    log_level: log::Level,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", format!("{},hyper=info,mio=info", &cli.log_level))
    }
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(handler))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    log::info!("listening on http://{}", &cli.addr);
    axum::Server::bind(&cli.addr).serve(app.into_make_service()).await.unwrap();
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    greeting: String,
}

#[derive(Debug)]
struct Html<T: Template>(T);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(body) => {
                let headers =
                    [(http::header::CONTENT_TYPE, http::HeaderValue::from_static(T::MIME_TYPE))];

                (headers, body).into_response()
            }
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

async fn handler() -> Html<Index> {
    Html(Index { greeting: "Hello, world".to_string() })
}

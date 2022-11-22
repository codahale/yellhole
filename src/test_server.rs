use std::net::{SocketAddr, TcpListener};

use axum::Router;
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, RequestBuilder, Url};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub struct TestServer {
    url: Url,
    client: Client,
}

impl TestServer {
    pub fn new(app: Router) -> Result<TestServer, anyhow::Error> {
        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "off".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .try_init();

        let listener = TcpListener::bind::<SocketAddr>(([127, 0, 0, 1], 0).into())?;
        let addr = listener.local_addr()?;
        let app = app.layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO).include_headers(true))
                .on_response(DefaultOnResponse::new().level(Level::INFO).include_headers(true)),
        );

        tokio::spawn(async move {
            axum::Server::from_tcp(listener).unwrap().serve(app.into_make_service()).await.unwrap();
        });

        Ok(TestServer {
            url: Url::parse(&format!("http://{addr}/"))?,
            client: ClientBuilder::new().redirect(Policy::none()).cookie_store(true).build()?,
        })
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.client.get(self.url.join(path).unwrap())
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.client.post(self.url.join(path).unwrap())
    }
}

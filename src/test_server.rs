use axum::Router;
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, RequestBuilder, Url};
use std::net::{SocketAddr, TcpListener};

pub struct TestServer {
    url: Url,
    client: Client,
}

impl TestServer {
    pub fn new(app: Router) -> Result<TestServer, anyhow::Error> {
        let listener = TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>()?)?;
        let addr = listener.local_addr()?;

        tokio::spawn(async move {
            axum::Server::from_tcp(listener).unwrap().serve(app.into_make_service()).await.unwrap();
        });

        Ok(TestServer {
            url: Url::parse(&format!("http://{addr}/"))?,
            client: ClientBuilder::new().redirect(Policy::none()).build()?,
        })
    }

    pub async fn get(&self, path: &str) -> Result<reqwest::Response, anyhow::Error> {
        Ok(self.client.get(self.url.join(path)?).send().await?)
    }

    pub fn post(&self, path: &str) -> Result<RequestBuilder, anyhow::Error> {
        Ok(self.client.post(self.url.join(path)?))
    }
}

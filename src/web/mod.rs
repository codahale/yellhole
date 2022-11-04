use std::net::SocketAddr;

use askama::Template;
use axum::http;
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

mod feed;

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
}

pub async fn serve(addr: &SocketAddr, db: SqlitePool) -> anyhow::Result<()> {
    let router = feed::router();

    let app = router
        .layer(AddExtensionLayer::new(Context { db }))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    log::info!("listening on http://{}", addr);
    axum::Server::bind(addr).serve(app.into_make_service()).await?;

    Ok(())
}

#[derive(Debug)]
pub struct Html<T: Template>(T);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(body) => {
                let headers =
                    [(http::header::CONTENT_TYPE, http::HeaderValue::from_static(T::MIME_TYPE))];

                (headers, body).into_response()
            }
            Err(_) => http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

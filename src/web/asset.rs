use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::{http, Router};
use include_dir::{include_dir, Dir};
use tokio::io;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn router(images_dir: impl AsRef<std::path::Path>) -> Router {
    Router::new()
        .route("/assets/*path", get(static_path))
        .nest_service(
            "/images",
            get_service(ServiceBuilder::new().service(ServeDir::new(images_dir)))
                .handle_error(io_error),
        )
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=31536000,immutable"),
        ))
}

async fn static_path(Path(path): Path<String>) -> Result<Response, StatusCode> {
    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_octet_stream();
    let content_type = http::HeaderValue::from_str(mime_type.as_ref()).expect("invalid header");
    let file = STATIC_DIR.get_file(path).ok_or(StatusCode::NOT_FOUND)?;
    Ok(([(http::header::CONTENT_TYPE, content_type)], file.contents()).into_response())
}

async fn io_error(err: io::Error) -> StatusCode {
    tracing::warn!(%err, "error handling static asset");
    StatusCode::INTERNAL_SERVER_ERROR
}

static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

#[cfg(test)]
mod tests {
    use crate::test_server::TestServer;

    use super::*;

    #[tokio::test]
    async fn static_asset() -> Result<(), anyhow::Error> {
        let ts = TestServer::new(router("."))?;

        let resp = ts.get("/assets/css/mvp-1.12.css").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/css")),
        );

        Ok(())
    }

    #[tokio::test]
    async fn image() -> Result<(), anyhow::Error> {
        let ts = TestServer::new(router("."))?;

        let resp = ts.get("/images/LICENSE").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }
}

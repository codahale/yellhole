use axum::extract::Path;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, get_service};
use axum::{http, middleware, Router};
use include_dir::{include_dir, Dir};
use tokio::io;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use super::app::AppState;
use super::AppError;

pub fn router(images_dir: impl AsRef<std::path::Path>) -> Router<AppState> {
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
        .layer(middleware::from_fn(not_found))
}

#[tracing::instrument(err)]
async fn static_path(Path(path): Path<String>) -> Result<Response, StatusCode> {
    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_octet_stream();
    let content_type = http::HeaderValue::from_str(mime_type.as_ref()).expect("invalid header");
    let file = STATIC_DIR.get_file(path).ok_or(StatusCode::NOT_FOUND)?;
    Ok(([(http::header::CONTENT_TYPE, content_type)], file.contents()).into_response())
}

#[tracing::instrument(level = "warn")]
async fn io_error(err: io::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn not_found<B>(req: Request<B>, next: Next<B>) -> Result<Response, AppError> {
    let resp = next.run(req).await;
    dbg!(resp.status());
    if resp.status() == StatusCode::NOT_FOUND {
        Err(AppError::NotFound)
    } else {
        Ok(resp)
    }
}

static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::test_server::TestEnv;

    use super::*;

    #[sqlx::test]
    async fn static_asset(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router("."))?;

        let resp = ts.get("/assets/css/mvp-1.12.css").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/css")),
        );

        Ok(())
    }

    #[sqlx::test]
    async fn image(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router("."))?;

        let resp = ts.get("/images/LICENSE").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }
}

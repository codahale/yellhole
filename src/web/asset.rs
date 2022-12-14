use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get_service;
use axum::{http, middleware, Router};
use tokio::io;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::services::assets::AssetService;
use crate::services::images::ImageService;

use super::app::AppState;
use super::AppError;

pub fn router(images: &ImageService, assets: &AssetService) -> io::Result<Router<AppState>> {
    let assets = get_service(
        ServiceBuilder::new()
            .service(ServeDir::new(assets.assets_dir()).precompressed_br().precompressed_gzip()),
    )
    .handle_error(io_error);

    Ok(Router::new()
        // Serve particular asset files.
        .route_service("/android-chrome-192x192.png", assets.clone())
        .route_service("/android-chrome-512x512.png", assets.clone())
        .route_service("/apple-touch-icon.png", assets.clone())
        .route_service("/favicon-16x16.png", assets.clone())
        .route_service("/favicon-32x32.png", assets.clone())
        .route_service("/favicon.ico", assets.clone())
        .route_service("/site.webmanifest", assets.clone())
        // Serve general asset files.
        .nest_service("/assets", assets)
        // Serve images.
        .nest_service(
            "/images",
            get_service(ServiceBuilder::new().service(ServeDir::new(images.images_dir())))
                .handle_error(io_error),
        )
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=31536000,immutable"),
        ))
        .layer(middleware::from_fn(not_found)))
}

#[tracing::instrument(level = "warn")]
async fn io_error(err: io::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn not_found<B>(req: Request<B>, next: Next<B>) -> Result<Response, AppError> {
    let resp = next.run(req).await;
    if resp.status() == StatusCode::NOT_FOUND {
        Err(AppError::NotFound)
    } else {
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sqlx::SqlitePool;

    use crate::test::TestEnv;

    use super::*;

    #[sqlx::test]
    async fn static_asset(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?;
        let app = router(&ts.state.images, &ts.state.assets)?;
        let ts = ts.into_server(app)?;

        let resp =
            ts.get("/assets/css/pico-1.5.6.min.css").header("Accept-Encoding", "br").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/css")),
        );
        assert_eq!(
            resp.headers().get(http::header::CONTENT_ENCODING),
            Some(&http::HeaderValue::from_static("br")),
        );

        Ok(())
    }

    #[sqlx::test]
    async fn image(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?;
        fs::copy("./yellhole.webp", ts.state.images.images_dir().join("yellhole.webp")).unwrap();
        let app = router(&ts.state.images, &ts.state.assets)?;
        let ts = ts.into_server(app)?;

        let resp = ts.get("/images/yellhole.webp").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }
}

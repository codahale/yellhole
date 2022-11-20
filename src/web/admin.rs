use askama::Template;
use axum::extract::{DefaultBodyLimit, Multipart};
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use serde::Deserialize;
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;
use url::Url;

use crate::services::images::{Image, ImageService};
use crate::services::notes::NoteService;

use super::Page;

pub fn router() -> Router {
    Router::new()
        .route("/admin/new", get(new_page))
        .route("/admin/new-note", post(create_note))
        .route("/admin/upload-images", post(upload_images))
        .route("/admin/download-image", post(download_image))
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::disable())
                .layer(RequestBodyLimitLayer::new(32 * 1024 * 1024)),
        )
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {
    images: Vec<Image>,
}

async fn new_page(images: Extension<ImageService>) -> Result<Page<NewPage>, StatusCode> {
    let images = images.most_recent(10).await.map_err(|err| {
        tracing::warn!(%err, "unable to query recent images");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Page(NewPage { images }))
}

#[derive(Debug, Deserialize)]
struct NewNote {
    body: String,
}

async fn create_note(
    notes: Extension<NoteService>,
    Form(new_note): Form<NewNote>,
) -> Result<Redirect, StatusCode> {
    let note_id = notes.create(&new_note.body).await.map_err(|err| {
        tracing::warn!(%err, "error inserting note");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Redirect::to(&format!("/note/{note_id}")))
}

pub async fn upload_images(
    images: Extension<ImageService>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        if let Some(content_type) =
            field.content_type().and_then(|ct| ct.parse::<mime::Mime>().ok())
        {
            if content_type.type_() == mime::IMAGE {
                let original_filename = field.file_name().unwrap_or("none").to_string();
                images.add(&original_filename, &content_type, field).await.map_err(|err| {
                    tracing::warn!(%err, "unable to add image");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            }
        }
    }
    Ok(Redirect::to("/admin/new"))
}

#[derive(Debug, Deserialize)]
struct DownloadImage {
    url: String,
}

async fn download_image(
    images: Extension<ImageService>,
    Form(image): Form<DownloadImage>,
) -> Result<Redirect, StatusCode> {
    let url = image.url.parse::<Url>().map_err(|err| {
        tracing::warn!(%err, "invalid URL");
        StatusCode::BAD_REQUEST
    })?;

    images.download(url).await.map_err(|err| {
        tracing::warn!(%err, "unable to download image");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Redirect::to("/admin/new"))
}

#[cfg(test)]
mod tests {
    use axum::http;
    use reqwest::multipart;
    use sqlx::SqlitePool;
    use tempdir::TempDir;
    use tokio::fs;
    use uuid::Uuid;

    use crate::test_server::TestServer;

    use super::*;

    #[sqlx::test(fixtures("notes", "images"))]
    async fn new_note_ui(db: SqlitePool) -> Result<(), anyhow::Error> {
        let temp_dir = TempDir::new("yellhole-test")?;
        let (_, _, app) = app(&db, &temp_dir)?;
        let ts = TestServer::new(app)?;

        let resp = ts.get("/admin/new").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("/images/cbdc5a69-abba-4d75-9679-44259c48b272.thumb.webp"));

        Ok(())
    }

    #[sqlx::test]
    async fn creating_a_note(db: SqlitePool) -> Result<(), anyhow::Error> {
        let temp_dir = TempDir::new("yellhole-test")?;
        let (_, notes, app) = app(&db, &temp_dir)?;
        let ts = TestServer::new(app)?;

        let resp = ts.post("/admin/new-note").form(&[("body", "This is a note.")]).send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp.headers().get(http::header::LOCATION).expect("missing header");
        let note_id = location.to_str()?.split('/').last().expect("bad URI").parse::<Uuid>()?;

        assert_eq!(notes.most_recent(20).await?.len(), 1);
        let note = notes.by_id(&note_id).await?.expect("missing note");
        assert_eq!(note.body, "This is a note.");

        Ok(())
    }

    #[sqlx::test]
    async fn uploading_an_image(db: SqlitePool) -> Result<(), anyhow::Error> {
        let temp_dir = TempDir::new("yellhole-test")?;
        let (images, _, app) = app(&db, &temp_dir)?;
        let ts = TestServer::new(app)?;

        let img = fs::read("yellhole.webp").await?;
        let form = multipart::Form::new().part(
            "one",
            multipart::Part::bytes(img).file_name("example.webp").mime_str("image/webp")?,
        );
        let resp = ts.post("/admin/upload-images").multipart(form).send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);

        let recent = images.most_recent(1).await?;
        assert_eq!(1, recent.len());

        Ok(())
    }

    #[sqlx::test]
    async fn downloading_an_image(db: SqlitePool) -> Result<(), anyhow::Error> {
        let temp_dir = TempDir::new("yellhole-test")?;
        let (images, _, app) = app(&db, &temp_dir)?;
        let ts = TestServer::new(app)?;

        let resp = ts
            .post("/admin/download-image")
            .form(&[("url", "https://crates.io/assets/Cargo-Logo-Small.png")])
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);

        let recent = images.most_recent(1).await?;
        assert_eq!(1, recent.len());

        Ok(())
    }

    fn app(
        db: &SqlitePool,
        temp_dir: &TempDir,
    ) -> Result<(ImageService, NoteService, Router), anyhow::Error> {
        let images = ImageService::new(db.clone(), temp_dir)?;
        let notes = NoteService::new(db.clone());
        Ok((
            images.clone(),
            notes.clone(),
            router().layer(Extension(images)).layer(Extension(notes)),
        ))
    }
}

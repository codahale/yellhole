use anyhow::Context;
use askama::Template;
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use chrono::Utc;
use mime::Mime;
use serde::Deserialize;
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;
use url::Url;
use uuid::Uuid;

use crate::{
    services::{images::Image, notes::Note},
    web::app::{AppError, AppState, Page},
};

pub fn router() -> Router<AppState> {
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

async fn new_page(state: State<AppState>) -> Result<Page<NewPage>, AppError> {
    Ok(Page(NewPage { images: state.images.most_recent(10).await? }))
}

#[derive(Debug, Deserialize)]
struct NewNote {
    body: String,
    preview: bool,
}

#[derive(Debug, Template)]
#[template(path = "preview.html")]
struct PreviewPage {
    note: Note,
}

async fn create_note(
    state: State<AppState>,
    Form(new_note): Form<NewNote>,
) -> Result<Response, AppError> {
    if new_note.preview {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: new_note.body,
            created_at: Utc::now(),
        };
        Ok(Page(PreviewPage { note }).into_response())
    } else {
        let note_id = state.notes.create(&new_note.body).await?;
        Ok(Redirect::to(&format!("/note/{note_id}")).into_response())
    }
}

async fn upload_images(
    state: State<AppState>,
    mut multipart: Multipart,
) -> Result<Redirect, AppError> {
    while let Some(field) = multipart.next_field().await.context("multipart error")? {
        if let Some(content_type) = field.content_type().and_then(|s| s.parse::<Mime>().ok()) {
            if content_type.type_() == mime::IMAGE {
                let original_filename = field.file_name().unwrap_or("none").to_string();
                state.images.add(&original_filename, &content_type, field).await?;
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
    state: State<AppState>,
    Form(image): Form<DownloadImage>,
) -> Result<Response, AppError> {
    if let Ok(url) = image.url.parse::<Url>() {
        state.images.download(url).await?;
        Ok(Redirect::to("/admin/new").into_response())
    } else {
        Ok(StatusCode::BAD_REQUEST.into_response())
    }
}

#[cfg(test)]
mod tests {
    use axum::routing::get_service;
    use reqwest::multipart;
    use sqlx::SqlitePool;
    use tokio::fs;
    use tower_http::services::ServeFile;
    use uuid::Uuid;

    use crate::test::TestEnv;

    use super::*;

    #[sqlx::test(fixtures("notes", "images"))]
    async fn new_note_ui(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router()).await?;

        let resp = ts.get("/admin/new").send().await?;
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("/images/cbdc5a69-abba-4d75-9679-44259c48b272.thumb.webp"));

        Ok(())
    }

    #[sqlx::test]
    async fn creating_a_note(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router()).await?;

        let resp = ts
            .post("/admin/new-note")
            .form(&[("body", "This is a note."), ("preview", "false")])
            .send()
            .await?;
        assert_eq!(resp.status(), reqwest::StatusCode::SEE_OTHER);
        let location = resp.headers().get(reqwest::header::LOCATION).expect("missing header");
        let note_id = location.to_str()?.split('/').last().expect("bad URI").parse::<Uuid>()?;

        assert_eq!(ts.state.notes.most_recent(20).await?.len(), 1);
        let note = ts.state.notes.by_id(&note_id).await?.expect("missing note");
        assert_eq!(note.body, "This is a note.");

        Ok(())
    }

    #[sqlx::test]
    async fn uploading_an_image(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router()).await?;

        let img = fs::read("yellhole.webp").await?;
        let form = multipart::Form::new().part(
            "one",
            multipart::Part::bytes(img).file_name("example.webp").mime_str("image/webp")?,
        );
        let resp = ts.post("/admin/upload-images").multipart(form).send().await?;
        assert_eq!(resp.status(), reqwest::StatusCode::SEE_OTHER);

        let recent = ts.state.images.most_recent(1).await?;
        assert_eq!(recent.len(), 1);

        Ok(())
    }

    #[sqlx::test]
    async fn downloading_an_image(db: SqlitePool) -> Result<(), anyhow::Error> {
        fn app() -> Router<AppState> {
            Router::new()
                .route_service("/logo.webp", get_service(ServeFile::new("yellhole.webp")))
                .merge(router())
        }

        let ts = TestEnv::new(db)?.into_server(app()).await?;

        let resp = ts
            .post("/admin/download-image")
            .form(&[("url", ts.url.join("/logo.webp")?.to_string())])
            .send()
            .await?;
        assert_eq!(resp.status(), reqwest::StatusCode::SEE_OTHER);

        let recent = ts.state.images.most_recent(1).await?;
        assert_eq!(recent.len(), 1);

        Ok(())
    }
}

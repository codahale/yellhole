use askama::Template;
use axum::body::BoxBody;
use axum::extract::Multipart;
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::models::{Image, Note};

use super::{CacheControl, Context, Html, WebError};

pub fn router() -> Router {
    // TODO add authentication
    Router::new()
        .route("/admin/new", get(new_page))
        .route("/admin/new-note", post(create_note))
        .route("/admin/upload-image", post(upload_image))
}

async fn new_page() -> Html<NewPage> {
    Html(NewPage {}, CacheControl::NoCache)
}

#[derive(Debug, Deserialize)]
struct NewNote {
    body: String,
}

async fn create_note(
    ctx: Extension<Context>,
    Form(new_note): Form<NewNote>,
) -> Result<Response, WebError> {
    let id = Note::create(&ctx.db, &new_note.body).await?;
    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header("location", format!("/note/{id}"))
        .body(BoxBody::default())
        .unwrap())
}

pub async fn upload_image(
    ctx: Extension<Context>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, WebError> {
    while let Some(field) = multipart.next_field().await.expect("bad request") {
        if let Some(content_type) =
            field.content_type().and_then(|ct| ct.parse::<mime::Mime>().ok())
        {
            if content_type.type_() == mime::IMAGE {
                // 1. create unprocessed image in DB, get image ID
                let original_ext = content_type.subtype().as_str();
                let image_id = Image::create(&ctx.db, original_ext).await?;
                let images_dir = ctx.images_dir();

                // 2. write image to dir/images/{image_id}.orig.{ext}
                let original_path = Image::original_path(&images_dir, &image_id, original_ext);
                let mut f = tokio::fs::File::create(&original_path).await?;
                f.write_all_buf(&mut field.bytes().await?).await?;

                // 3. process image, generating thumbnail etc. in parallel
                let main_path = Image::main_path(&images_dir, &image_id);
                let main = Image::process_image(original_path.clone(), main_path, "600");

                let thumbnail_path = Image::thumbnail_path(&images_dir, &image_id);
                let thumbnail = Image::process_image(original_path.clone(), thumbnail_path, "100");

                main.await?;
                thumbnail.await?;

                // 4. mark image as processed
                Image::mark_processed(&ctx.db, &image_id).await?;
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(http::header::LOCATION, "/")
        .body(BoxBody::default())
        .unwrap())
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {}

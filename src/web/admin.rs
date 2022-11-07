use std::path::PathBuf;
use std::process::ExitStatus;

use askama::Template;
use axum::body::BoxBody;
use axum::extract::Multipart;
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use serde::Deserialize;
use tokio::io::{self, AsyncWriteExt};
use tokio::process::Command;

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

                // 2. write image to dir/images/{image_id}.orig.{ext}
                let mut images_path = ctx.dir.clone();
                images_path.push("images");

                let mut path = images_path.clone();
                path.push(format!("{image_id}.orig.{original_ext}"));
                let mut f = tokio::fs::File::create(&path).await.expect("bad file");
                f.write_all_buf(&mut field.bytes().await.expect("bad request"))
                    .await
                    .expect("bad write");

                // 3. process image, generating thumbnail etc. in parallel
                let mut main = images_path.clone();
                main.push(format!("{image_id}.main.webp"));

                let mut thumbnail = images_path.clone();
                thumbnail.push(format!("{image_id}.thumbnail.webp"));

                let a = process_image(path.clone(), main, "600");
                let b = process_image(path.clone(), thumbnail, "100");

                let (a, b) = futures::join!(a, b);
                a?;
                b?;

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

async fn process_image(
    input: PathBuf,
    output: PathBuf,
    geometry: &'static str,
) -> io::Result<ExitStatus> {
    let mut proc = Command::new("magick")
        .arg(input)
        .arg("-auto-orient")
        .arg("-strip")
        .arg("-thumbnail")
        .arg(geometry)
        .arg(output)
        .spawn()?;
    proc.wait().await
}

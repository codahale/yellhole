use std::path::Path;

use askama::Template;
use axum::body::{BoxBody, Bytes};
use axum::extract::Multipart;
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{BoxError, Extension, Form, Router};
use futures::{Stream, TryStreamExt};
use serde::Deserialize;
use tokio::fs::File;
use tokio::io::{self, BufWriter};
use tokio_util::io::StreamReader;

use crate::models::{Image, Note};

use super::{CacheControl, Context, Html};

pub fn router() -> Router {
    // TODO add authentication
    Router::new()
        .route("/admin/new", get(new_page))
        .route("/admin/new-note", post(create_note))
        .route("/admin/upload-images", post(upload_images))
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {
    images: Vec<Image>,
}

async fn new_page(ctx: Extension<Context>) -> Result<Html<NewPage>, StatusCode> {
    let images = Image::most_recent(&ctx.db, 10).await.map_err(|err| {
        tracing::warn!(%err, "unable to query recent images");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Html(NewPage { images }, CacheControl::NoCache))
}

#[derive(Debug, Deserialize)]
struct NewNote {
    body: String,
}

async fn create_note(
    ctx: Extension<Context>,
    Form(new_note): Form<NewNote>,
) -> Result<Response, StatusCode> {
    let id = Note::create(&ctx.db, &new_note.body).await.map_err(|err| {
        tracing::warn!(%err, "error inserting note");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header("location", format!("/note/{id}"))
        .body(BoxBody::default())
        .unwrap())
}

pub async fn upload_images(
    ctx: Extension<Context>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, StatusCode> {
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        if let Some(content_type) =
            field.content_type().and_then(|ct| ct.parse::<mime::Mime>().ok())
        {
            if content_type.type_() == mime::IMAGE {
                // 1. create unprocessed image in DB, get image ID
                let image_id = Image::create(&ctx.db, &content_type).await.map_err(|err| {
                    tracing::warn!(%err, "error inserting image");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

                // 2. write image to dir/uploads/{image_id}.orig.{ext}
                let original_path =
                    Image::original_path(&ctx.uploads_dir, &image_id, &content_type);
                stream_to_file(&original_path, field).await.map_err(|err| {
                    tracing::warn!(%err, image_id, "error receiving image");
                    StatusCode::BAD_REQUEST
                })?;

                // 3. process image, generating thumbnail etc. in parallel
                let main_path = Image::main_path(&ctx.images_dir, &image_id);
                let main = Image::process_image(original_path.clone(), main_path, "600");

                let thumbnail_path = Image::thumbnail_path(&ctx.images_dir, &image_id);
                let thumbnail = Image::process_image(original_path.clone(), thumbnail_path, "100");

                main.await.map_err(|err| {
                    tracing::warn!(%err, image_id, "error generating image");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                thumbnail.await.map_err(|err| {
                    tracing::warn!(%err, image_id, "error generating thumbnail");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

                // 4. mark image as processed
                Image::mark_processed(&ctx.db, &image_id).await.map_err(|err| {
                    tracing::warn!(%err, image_id, "error updating image");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(http::header::LOCATION, "/admin/new")
        .body(BoxBody::default())
        .unwrap())
}

async fn stream_to_file<S, E>(path: &Path, stream: S) -> Result<(), io::Error>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    // Convert the stream into an `AsyncRead`.
    let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let body_reader = StreamReader::new(body_with_io_error);
    futures::pin_mut!(body_reader);

    // Create the file. `File` implements `AsyncWrite`.
    let mut file = BufWriter::new(File::create(path).await?);

    // Copy the body into the file.
    tokio::io::copy(&mut body_reader, &mut file).await?;

    Ok(())
}

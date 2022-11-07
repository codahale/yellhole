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

use super::{CacheControl, Context, Html, WebError};

pub fn router() -> Router {
    // TODO add authentication
    Router::new()
        .route("/admin/new", get(new_page))
        .route("/admin/new-note", post(create_note))
        .route("/admin/upload-image", post(upload_image))
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {}

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
                stream_to_file(&original_path, field).await?;

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

async fn stream_to_file<S, E>(path: &Path, stream: S) -> Result<(), WebError>
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

use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use askama::Template;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Multipart};
use axum::http::{self, StatusCode};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{BoxError, Extension, Form, Router};
use futures::{Stream, TryStreamExt};
use mime::Mime;
use serde::Deserialize;
use tokio::fs::File;
use tokio::io::{self, BufWriter};
use tokio::process::Command;
use tokio_util::io::StreamReader;
use tower::ServiceBuilder;
use tower_http::auth::RequireAuthorizationLayer;
use tower_http::limit::RequestBodyLimitLayer;
use url::Url;
use uuid::Uuid;

use crate::models::{Image, Note};

use super::{Context, Page};

pub fn router(password: &str) -> Router {
    Router::new()
        .route("/admin/new", get(new_page))
        .route("/admin/new-note", post(create_note))
        .route("/admin/upload-images", post(upload_images))
        .route("/admin/download-image", post(download_image))
        .layer(
            ServiceBuilder::new()
                .layer(RequireAuthorizationLayer::basic("admin", password))
                .layer(DefaultBodyLimit::disable()),
        )
        .layer(RequestBodyLimitLayer::new(32 * 1024 * 1024))
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {
    images: Vec<Image>,
}

async fn new_page(ctx: Extension<Context>) -> Result<Page<NewPage>, StatusCode> {
    let images = Image::most_recent(&ctx.db, 10).await.map_err(|err| {
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
    ctx: Extension<Context>,
    Form(new_note): Form<NewNote>,
) -> Result<Redirect, StatusCode> {
    let note_id = Uuid::new_v4();
    Note::create(&ctx.db, &note_id.to_string(), &new_note.body).await.map_err(|err| {
        tracing::warn!(%err, "error inserting note");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Redirect::to(&format!("/note/{note_id}")))
}

pub async fn upload_images(
    ctx: Extension<Context>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        if let Some(content_type) =
            field.content_type().and_then(|ct| ct.parse::<mime::Mime>().ok())
        {
            if content_type.type_() == mime::IMAGE {
                let original_filename = field.file_name().unwrap_or("none").to_string();
                add_image(&ctx, &original_filename, &content_type, field).await?;
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
    ctx: Extension<Context>,
    Form(image): Form<DownloadImage>,
) -> Result<Redirect, StatusCode> {
    // Parse the URL to see if it's valid.
    let url = image.url.parse::<Url>().map_err(|err| {
        tracing::warn!(%err, "invalid URL");
        StatusCode::BAD_REQUEST
    })?;
    let original_filename = url.to_string();

    // Start the request to download the image.
    let image = reqwest::get(url).await.map_err(|err| {
        tracing::warn!(%err, "error downloading image");
        StatusCode::GATEWAY_TIMEOUT
    })?;

    // Get the image's content type.
    let Some(content_type) = image
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<mime::Mime>().ok()) else {
        tracing::warn!("error determining image content type");
        return Err(StatusCode::GATEWAY_TIMEOUT);
        };

    let stream = image.bytes_stream();
    add_image(&ctx, &original_filename, &content_type, stream).await?;

    Ok(Redirect::to("/admin/new"))
}

async fn add_image<S, E>(
    ctx: &Extension<Context>,
    original_filename: &str,
    content_type: &Mime,
    stream: S,
) -> Result<String, StatusCode>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    // 1. create an image ID
    let image_id = Uuid::new_v4().to_string();

    // 2. write image to dir/uploads/{image_id}.orig.{ext}
    let original_path = Image::original_path(&ctx.uploads_dir, &image_id, content_type);
    stream_to_file(&original_path, stream).await.map_err(|err| {
        tracing::warn!(%err, image_id, "error downloading image");
        StatusCode::GATEWAY_TIMEOUT
    })?;

    // 3. process image, generating thumbnail etc. in parallel
    let main_path = Image::main_path(&ctx.images_dir, &image_id);
    let main = process_image(original_path.clone(), main_path, "600");

    let thumbnail_path = Image::thumbnail_path(&ctx.images_dir, &image_id);
    let thumbnail = process_image(original_path.clone(), thumbnail_path, "100");

    main.await.map_err(|err| {
        tracing::warn!(%err, image_id, "error generating image");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    thumbnail.await.map_err(|err| {
        tracing::warn!(%err, image_id, "error generating thumbnail");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 4. Insert image into DB.
    Image::create(&ctx.db, &image_id, original_filename, content_type).await.map_err(|err| {
        tracing::warn!(%err, image_id, "error creating image");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(image_id)
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

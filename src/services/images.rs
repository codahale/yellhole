use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::Context;
use axum::body::Bytes;
use axum::{http, BoxError};
use chrono::NaiveDateTime;
use futures::{Stream, TryStreamExt};
use mime::Mime;
use sqlx::SqlitePool;
use tokio::fs::File;
use tokio::io::{self, BufWriter};
use tokio::process::Command;
use tokio_util::io::StreamReader;
use url::Url;
use uuid::fmt::Hyphenated;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ImageService {
    db: SqlitePool,
    data_dir: PathBuf,
}

impl ImageService {
    pub fn new(db: SqlitePool, data_dir: impl AsRef<Path>) -> Result<ImageService, io::Error> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(data_dir.join(IMAGES_DIR))?;
        fs::create_dir_all(data_dir.join(UPLOADS_DIR))?;
        Ok(ImageService { db, data_dir })
    }

    /// Returns the `n` most recent images, in reverse chronological order.
    pub async fn most_recent(&self, n: u16) -> Result<Vec<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r#"
            select image_id as "image_id: Hyphenated", created_at
            from image
            order by created_at desc
            limit ?
            "#,
            n
        )
        .fetch_all(&self.db)
        .await
    }

    /// Processes the given stream as an image file and adds it to the database. Generates a main
    /// WebP image for displaying in the feed and a thumbnail WebP image for the new note gallery.
    pub async fn add<S, E>(
        &self,
        original_filename: &str,
        content_type: &Mime,
        stream: S,
    ) -> Result<Hyphenated, anyhow::Error>
    where
        S: Stream<Item = Result<Bytes, E>>,
        E: Into<BoxError>,
    {
        // Create a unique ID for the image.
        let image_id = Uuid::new_v4().hyphenated();

        // Stream the image file to the uploads directory.
        let original_path = self
            .data_dir
            .join(UPLOADS_DIR)
            .join(format!("{image_id}.orig.{}", content_type.subtype()));
        stream_to_file(stream, &original_path).await.context("error streaming image")?;

        // Generate a 600px-wide main WebP image.
        let main_path = self.data_dir.join(IMAGES_DIR).join(main_filename(&image_id));
        let main = process_image(&original_path, &main_path, "600");

        // Generate a 100px-wide thumbnail WebP image.
        let thumbnail_path = self.data_dir.join(IMAGES_DIR).join(thumbnail_filename(&image_id));
        let thumbnail = process_image(&original_path, &thumbnail_path, "100");

        // Wait for image processing to complete.
        main.await.context("error generating main image")?;
        thumbnail.await.context("error generating thumbnail image")?;

        // Add image to the database.
        let content_type = content_type.to_string();
        sqlx::query!(
            r"insert into image (image_id, original_filename, content_type) values (?, ?, ?)",
            image_id,
            original_filename,
            content_type
        )
        .execute(&self.db)
        .await?;

        Ok(image_id)
    }

    pub async fn download(&self, image_url: Url) -> Result<Hyphenated, anyhow::Error> {
        let original_filename = image_url.to_string();

        // Start the request to download the image.
        let image = reqwest::get(image_url).await.context("error downloading image")?;

        // Get the image's content type.
        let content_type = image
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("no Content-Type header"))
            .and_then(|s| s.parse::<Mime>().context("invalid Content-Type header"))?;

        // Add the response body as an image.
        self.add(&original_filename, &content_type, image.bytes_stream()).await
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Image {
    image_id: Hyphenated,
    pub created_at: NaiveDateTime,
}

impl Image {
    /// The URI for the main version of the image.
    pub fn main_src(&self) -> String {
        format!("/{}/{}", IMAGES_DIR, main_filename(&self.image_id))
    }

    /// The URI for the thumbnail version of the image.
    pub fn thumbnail_src(&self) -> String {
        format!("/{}/{}", IMAGES_DIR, thumbnail_filename(&self.image_id))
    }
}

/// The canonical filename of the main version of an image.
fn main_filename(image_id: &Hyphenated) -> String {
    format!("{}.main.webp", image_id)
}

/// The canonical filename of the thumbnail version of an image.
fn thumbnail_filename(image_id: &Hyphenated) -> String {
    format!("{}.thumb.webp", image_id)
}

async fn stream_to_file<S, E>(stream: S, path: &Path) -> Result<(), io::Error>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    // Convert the stream into an `AsyncRead`.
    let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
    let body_reader = StreamReader::new(body_with_io_error);
    futures::pin_mut!(body_reader);

    // Create the file.
    let mut file = BufWriter::new(File::create(path).await?);

    // Copy the body into the file.
    tokio::io::copy(&mut body_reader, &mut file).await?;

    Ok(())
}

async fn process_image<'a>(
    input: &'a Path,
    output: &'a Path,
    geometry: &'static str,
) -> io::Result<ExitStatus> {
    let mut proc = Command::new("convert")
        .arg(input)
        .arg("-auto-orient")
        .arg("-strip")
        .arg("-thumbnail")
        .arg(geometry)
        .arg(output)
        .spawn()?;
    proc.wait().await
}

const UPLOADS_DIR: &str = "uploads";

const IMAGES_DIR: &str = "images";

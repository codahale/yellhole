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
        fs::create_dir_all(data_dir.join("images"))?;
        fs::create_dir_all(data_dir.join("uploads"))?;
        Ok(ImageService { db, data_dir })
    }

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
        // 1. create an image ID
        let image_id = Uuid::new_v4().hyphenated();

        // 2. write image to dir/uploads/{image_id}.orig.{ext}
        let original_path = self
            .data_dir
            .join("uploads")
            .join(format!("{image_id}.orig.{}", content_type.subtype()));
        stream_to_file(&original_path, stream).await.context("error streaming image")?;

        // 3. process image, generating thumbnail etc. in parallel
        let main_path = self.data_dir.join("images").join(format!("{image_id}.main.webp"));
        let main = process_image(original_path.clone(), main_path, "600");

        let thumbnail_path = self.data_dir.join("images").join(format!("{image_id}.thumb.webp"));
        let thumbnail = process_image(original_path.clone(), thumbnail_path, "100");

        main.await.context("error generating main image")?;
        thumbnail.await.context("error generating thumbnail image")?;

        // 4. Insert image into DB.
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

        self.add(&original_filename, &content_type, image.bytes_stream()).await
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Image {
    pub image_id: Hyphenated,
    pub created_at: NaiveDateTime,
}

impl Image {
    pub fn main_src(&self) -> String {
        format!("/images/{}.main.webp", &self.image_id)
    }

    pub fn thumbnail_src(&self) -> String {
        format!("/images/{}.thumb.webp", &self.image_id)
    }
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

#[cfg(test)]
mod tests {
    use tempdir::TempDir;

    use super::*;

    #[sqlx::test(fixtures("images"))]
    async fn most_recent(db: SqlitePool) -> Result<(), anyhow::Error> {
        let temp_dir = TempDir::new("yellhole")?;
        let images = ImageService::new(db, temp_dir)?;
        let top_2 = images.most_recent(2).await?;
        assert_eq!(2, top_2.len());
        assert_eq!("4c89cfef-9031-49c0-8b91-2578c0e227f3", &top_2[0].image_id.to_string());
        assert_eq!("7963d8bc-9cf8-4459-a593-b6d49b94b541", &top_2[1].image_id.to_string());

        Ok(())
    }
}

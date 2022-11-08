use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use sqlx::SqlitePool;

#[derive(Debug)]
pub struct Image {
    pub image_id: String,
    pub created_at: NaiveDateTime,
}

impl Image {
    pub async fn most_recent(db: &SqlitePool, n: u16) -> Result<Vec<Image>, sqlx::Error> {
        sqlx::query_as!(
            Image,
            r"
            select image_id, created_at
            from image
            where processed
            order by created_at desc
            limit ?
            ",
            n
        )
        .fetch_all(db)
        .await
    }

    pub fn original_path(uploads_dir: &Path, image_id: &str, content_type: &mime::Mime) -> PathBuf {
        let mut path = uploads_dir.to_path_buf();
        path.push(format!("{image_id}.orig.{}", content_type.subtype()));
        path
    }

    pub fn main_path(images_dir: &Path, image_id: &str) -> PathBuf {
        let mut path = images_dir.to_path_buf();
        path.push(format!("{image_id}.main.webp"));
        path
    }

    pub fn thumbnail_path(images_dir: &Path, image_id: &str) -> PathBuf {
        let mut path = images_dir.to_path_buf();
        path.push(format!("{image_id}.thumb.webp"));
        path
    }

    pub async fn create(
        db: &SqlitePool,
        original_filename: &str,
        content_type: &mime::Mime,
    ) -> Result<String, sqlx::Error> {
        let content_type = content_type.to_string();
        sqlx::query!(
            r"
            insert into image (original_filename, content_type) values (?, ?) returning image_id
            ",
            original_filename,
            content_type
        )
        .fetch_one(db)
        .await
        .map(|r| r.image_id)
    }

    pub async fn mark_processed(db: &SqlitePool, image_id: &str) -> Result<(), sqlx::Error> {
        (sqlx::query!(
            r"
            update image set processed = true where image_id = ? 
            ",
            image_id
        )
        .execute(db)
        .await?
        .rows_affected()
            == 1)
            .then_some(())
            .ok_or(sqlx::Error::RowNotFound)
    }
}

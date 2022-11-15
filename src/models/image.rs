use chrono::NaiveDateTime;
use sqlx::SqlitePool;
use uuid::fmt::Hyphenated;

#[derive(Debug, PartialEq, Eq)]
pub struct Image {
    pub image_id: Hyphenated,
    pub created_at: NaiveDateTime,
}

impl Image {
    pub async fn most_recent(db: &SqlitePool, n: u16) -> Result<Vec<Image>, sqlx::Error> {
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
        .fetch_all(db)
        .await
    }

    pub async fn create(
        db: &SqlitePool,
        image_id: &Hyphenated,
        original_filename: &str,
        content_type: &mime::Mime,
    ) -> Result<(), sqlx::Error> {
        let content_type = content_type.to_string();
        sqlx::query!(
            r"
            insert into image (image_id, original_filename, content_type) values (?, ?, ?)
            ",
            image_id,
            original_filename,
            content_type
        )
        .execute(db)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[sqlx::test(fixtures("images"))]
    async fn most_recent(db: SqlitePool) -> Result<(), sqlx::Error> {
        let top_2 = Image::most_recent(&db, 2).await?;
        assert_eq!(2, top_2.len());
        assert_eq!("4c89cfef-9031-49c0-8b91-2578c0e227f3", &top_2[0].image_id.to_string());
        assert_eq!("7963d8bc-9cf8-4459-a593-b6d49b94b541", &top_2[1].image_id.to_string());

        Ok(())
    }

    #[sqlx::test]
    async fn round_trip(db: SqlitePool) -> Result<(), sqlx::Error> {
        let image_id = Uuid::new_v4();
        Image::create(&db, image_id.as_hyphenated(), "garfield-levitate.gif", &mime::IMAGE_GIF)
            .await?;

        let top = Image::most_recent(&db, 3).await?;
        assert_eq!(1, top.len());
        assert_eq!(&image_id, top[0].image_id.as_uuid());

        Ok(())
    }
}

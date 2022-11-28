use std::time::Duration;

use sqlx::SqlitePool;
use tokio::time;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionService {
    db: SqlitePool,
}

impl SessionService {
    pub const TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);

    pub fn new(db: SqlitePool) -> SessionService {
        SessionService { db }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn authenticate(&self) -> Result<Uuid, sqlx::Error> {
        let session_id = Uuid::new_v4();
        {
            let session_id = session_id.as_hyphenated().to_string();
            sqlx::query!(r#"insert into session (session_id) values (?)"#, session_id)
                .execute(&self.db)
                .await?;
        }

        Ok(session_id)
    }

    #[tracing::instrument(skip_all, ret, err)]
    pub async fn is_authenticated(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let authenticated = sqlx::query!(
            r#"
            select count(1) as n
            from session
            where session_id = ? and created_at > datetime('now', '-7 days')"#,
            session_id,
        )
        .fetch_one(&self.db)
        .await?
        .n > 0;

        Ok(authenticated)
    }

    pub async fn continuously_delete_expired(self) -> Result<(), sqlx::Error> {
        let mut interval = time::interval(Duration::from_secs(10 * 60));
        interval.tick().await; // skip immediate tick
        loop {
            interval.tick().await;
            self.delete_expired().await?;
        }
    }

    #[tracing::instrument(skip(self), ret, err)]
    async fn delete_expired(&self) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query!(r"delete from session where created_at < datetime('now', '-7 days')")
            .execute(&self.db)
            .await?
            .rows_affected())
    }
}

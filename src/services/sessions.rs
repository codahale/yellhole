use std::time::Duration;

use axum::async_trait;
use axum_sessions::async_session::{Result, Session, SessionStore};
use axum_sessions::{SameSite, SessionLayer};
use sqlx::SqlitePool;
use tokio::{task, time};
use url::Url;

#[derive(Debug, Clone)]
pub struct SessionService {
    db: SqlitePool,
}

impl SessionService {
    pub fn new(
        db: &SqlitePool,
        base_url: &Url,
    ) -> (SessionLayer<SessionService>, task::JoinHandle<anyhow::Result<()>>) {
        let store = SessionService { db: db.clone() };
        let session_expiry = task::spawn(store.clone().continuously_delete_expired());
        let session_layer = SessionLayer::new(store, &[69; 64])
            .with_cookie_name("yellhole")
            .with_same_site_policy(SameSite::Strict)
            .with_session_ttl(Some(Duration::from_secs(60 * 60 * 24 * 7)))
            .with_secure(base_url.scheme() == "https");
        (session_layer, session_expiry)
    }

    pub async fn continuously_delete_expired(self) -> Result<()> {
        let mut interval = time::interval(Duration::from_secs(10 * 60));
        interval.tick().await; // skip immediate tick
        loop {
            interval.tick().await;
            self.delete_expired().await?;
        }
    }

    async fn delete_expired(&self) -> Result<()> {
        tracing::trace!("destroying expired sessions");
        sqlx::query!(r"delete from session where updated_at < datetime('now', '-7 days')")
            .execute(&self.db)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for SessionService {
    #[tracing::instrument(skip(self), level = "trace", err)]
    async fn load_session(&self, cookie_value: String) -> Result<Option<Session>> {
        let session_id = Session::id_from_cookie_value(&cookie_value)?;
        Ok(sqlx::query!(r"select as_json from session where session_id = ?", session_id)
            .fetch_optional(&self.db)
            .await?
            .map(|r| serde_json::from_str::<Session>(&r.as_json))
            .transpose()?)
    }

    #[tracing::instrument(skip(self, session), fields(id = session.id()), level = "trace", err)]
    async fn store_session(&self, session: Session) -> Result<Option<String>> {
        let json = serde_json::to_string(&session)?;
        let session_id = session.id();
        sqlx::query!(
            r"
            insert into session (session_id, as_json)
            values (?, ?)
            on conflict (session_id) do
            update set as_json = ?, updated_at = current_timestamp
            ",
            session_id,
            json,
            json,
        )
        .execute(&self.db)
        .await?;

        session.reset_data_changed();
        Ok(session.into_cookie_value())
    }

    #[tracing::instrument(skip(self, session), fields(id = session.id()), level = "trace", err)]
    async fn destroy_session(&self, session: Session) -> Result {
        let session_id = session.id();
        sqlx::query!(r"delete from session where session_id = ?", session_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), level = "trace", err)]
    async fn clear_store(&self) -> Result {
        sqlx::query!(r"delete from session").execute(&self.db).await?;
        Ok(())
    }
}

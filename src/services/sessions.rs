use std::time::Duration;

use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use tokio::{task, time};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionService {
    db: SqlitePool,
    secure: bool,
}

impl SessionService {
    const TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);

    pub fn new(
        db: SqlitePool,
        base_url: &Url,
    ) -> (SessionService, JoinHandle<Result<(), sqlx::Error>>) {
        let service = SessionService { db, secure: base_url.scheme() == "https" };
        let expiry = task::spawn(service.clone().continuously_delete_expired());
        (service, expiry)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn authenticate(&self, cookies: CookieJar) -> Result<CookieJar, sqlx::Error> {
        let session_id = Uuid::new_v4().as_hyphenated().to_string();
        let cookie = Cookie::build("session", session_id.clone())
            .http_only(true)
            .same_site(SameSite::Strict)
            .max_age(Self::TTL.try_into().expect("invalid duration"))
            .secure(self.secure)
            .path("/")
            .finish();

        sqlx::query!(r#"insert into session (session_id) values (?)"#, session_id)
            .execute(&self.db)
            .await?;

        Ok(cookies.add(cookie))
    }

    #[tracing::instrument(skip_all, ret, err)]
    pub async fn is_authenticated(&self, cookies: &CookieJar) -> Result<bool, sqlx::Error> {
        let Some(cookie) = cookies.get("session") else {
            return Ok(false);
        };

        let session_id = cookie.value();
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

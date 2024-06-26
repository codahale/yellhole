use std::time::Duration;

use rusqlite::params;
use tokio::time;
use tokio_rusqlite::Connection;

use crate::id::PublicId;

/// A service which manages authenticated sessions.
#[derive(Debug, Clone)]
pub struct SessionService {
    db: Connection,
}

impl SessionService {
    /// The duration of an authenticated session.
    pub const TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7);

    /// Creates a new [`SessionService`] with the given database.
    pub fn new(db: Connection) -> SessionService {
        SessionService { db }
    }

    /// Creates an authenticated session and returns its ID.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn create(&self) -> Result<PublicId, tokio_rusqlite::Error> {
        let session_id = PublicId::random();
        self.db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    insert into session (session_id)
                    values (?)
                    "#,
                )?
                .execute(params![session_id])
            })
            .await?;
        Ok(session_id)
    }

    /// Returns `true` if a session with the given ID exists.
    #[must_use]
    #[tracing::instrument(skip_all, ret, err)]
    pub async fn exists(&self, session_id: PublicId) -> Result<bool, tokio_rusqlite::Error> {
        Ok(self
            .db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    select count(1) > 0
                    from session
                    where session_id = ? and created_at > datetime('now', '-7 days')
                    "#,
                )?
                .query_row(params![session_id], |row| row.get(0))
            })
            .await?)
    }

    /// Runs an infinite asynchronous loop, deleting expired sessions every 10 minutes.
    pub async fn continuously_delete_expired(self) -> Result<(), tokio_rusqlite::Error> {
        let mut interval = time::interval(Duration::from_secs(10 * 60));
        interval.tick().await; // skip immediate tick
        loop {
            interval.tick().await;
            self.delete_expired().await?;
        }
    }

    #[must_use]
    #[tracing::instrument(skip(self), ret, err)]
    async fn delete_expired(&self) -> Result<usize, tokio_rusqlite::Error> {
        Ok(self
            .db
            .call_unwrap(|conn| {
                conn.prepare_cached(
                    r#"delete from session where created_at < datetime('now', '-7 days')"#,
                )?
                .raw_execute()
            })
            .await?)
    }
}

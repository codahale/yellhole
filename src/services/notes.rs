use std::ops::Range;

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use pulldown_cmark::{Event, Parser, Tag};
use sqlx::SqlitePool;
use url::Url;
use uuid::fmt::Hyphenated;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NoteService {
    db: SqlitePool,
}

impl NoteService {
    pub fn new(db: SqlitePool) -> NoteService {
        NoteService { db }
    }

    #[tracing::instrument(skip(self, body), ret(Display), err)]
    pub async fn create(&self, body: &str) -> Result<Hyphenated, sqlx::Error> {
        let note_id = Uuid::new_v4().hyphenated();
        sqlx::query!(r"insert into note (note_id, body) values (?, ?)", note_id, body)
            .execute(&self.db)
            .await?;
        Ok(note_id)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn by_id(&self, note_id: &Uuid) -> Result<Option<Note>, sqlx::Error> {
        let note_id = note_id.as_hyphenated();
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: DateTime<Utc>"
            from note
            where note_id = ?
            "#,
            note_id
        )
        .fetch_optional(&self.db)
        .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn most_recent(&self, n: u16) -> Result<Vec<Note>, sqlx::Error> {
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: DateTime<Utc>"
            from note
            order by created_at desc
            limit ?
            "#,
            n
        )
        .fetch_all(&self.db)
        .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn weeks(&self) -> Result<Vec<Range<NaiveDate>>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"
            select
              date(local, 'weekday 0', '-7 days') as "start!: NaiveDate",
              date(local, 'weekday 0') as "end!: NaiveDate"
            from (select datetime(created_at, 'localtime') as local from note)
            group by 1 order by 1 desc"#,
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| r.start..r.end)
        .collect())
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn date_range(&self, range: Range<NaiveDate>) -> Result<Vec<Note>, sqlx::Error> {
        let start = local_date_to_utc(&range.start);
        let end = local_date_to_utc(&range.end);
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: DateTime<Utc>"
            from note
            where created_at >= ? and created_at < ?
            order by created_at desc
            "#,
            start,
            end,
        )
        .fetch_all(&self.db)
        .await
    }
}

#[derive(Debug)]
pub struct Note {
    pub note_id: Hyphenated,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

impl Note {
    pub fn to_html(&self) -> String {
        let mut out = String::with_capacity(256);
        pulldown_cmark::html::push_html(&mut out, Parser::new(&self.body));
        out
    }

    pub fn images(&self, base_url: &Url) -> Vec<Url> {
        Parser::new(&self.body)
            .flat_map(|e| match e {
                Event::Start(Tag::Image(_, url, _)) => base_url.join(url.as_ref()).ok(),
                _ => None,
            })
            .collect()
    }

    pub fn description(&self) -> String {
        Parser::new(&self.body).fold(String::with_capacity(256), |mut d, e| {
            if let Event::Text(s) = e {
                d.push_str(s.as_ref());
            }
            d
        })
    }
}

fn local_date_to_utc(d: &NaiveDate) -> DateTime<Utc> {
    Local.from_local_datetime(&d.and_time(NaiveTime::default())).unwrap().with_timezone(&Utc)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn body_to_html() {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: r#"It's _electric_!"#.into(),
            created_at: Utc::now(),
        };

        assert_eq!(note.to_html(), "<p>It's <em>electric</em>!</p>\n");
    }

    #[test]
    fn body_to_description() {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: r#"It's _electric_!"#.into(),
            created_at: Utc::now(),
        };

        assert_eq!(note.description(), r#"It's electric!"#);
    }
}

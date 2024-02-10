use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};
use sqlx::SqlitePool;
use time::{Date, OffsetDateTime, Time};
use url::Url;
use uuid::{fmt::Hyphenated, Uuid};

/// A service for creating and viewing [`Note`]s.
#[derive(Debug, Clone)]
pub struct NoteService {
    db: SqlitePool,
}

impl NoteService {
    /// Create a new [`NoteService`] using the given database.
    pub fn new(db: SqlitePool) -> NoteService {
        NoteService { db }
    }

    /// Create a new [`Note`], returning the new note's ID.
    #[must_use]
    #[tracing::instrument(skip(self, body), ret(Display), err)]
    pub async fn create(&self, body: &str) -> Result<Hyphenated, sqlx::Error> {
        let note_id = Uuid::new_v4().hyphenated();
        sqlx::query!(r#"insert into note (note_id, body) values (?, ?)"#, note_id, body)
            .execute(&self.db)
            .await?;
        Ok(note_id)
    }

    /// Find a [`Note`] by ID.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn by_id(&self, note_id: &Uuid) -> Result<Option<Note>, sqlx::Error> {
        let note_id = note_id.as_hyphenated();
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: OffsetDateTime"
            from note
            where note_id = ?
            "#,
            note_id
        )
        .fetch_optional(&self.db)
        .await
    }

    /// Find the `n` most recent [`Note`]s in reverse chronological order.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn most_recent(&self, n: u16) -> Result<Vec<Note>, sqlx::Error> {
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: OffsetDateTime"
            from note
            order by created_at desc
            limit ?
            "#,
            n
        )
        .fetch_all(&self.db)
        .await
    }

    /// Return a vec of all week-long date ranges in which notes were created.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn weeks(&self) -> Result<Vec<Range<Date>>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"
            select
              date(local, 'weekday 0', '-7 days') as "start!: Date",
              date(local, 'weekday 0') as "end!: Date"
            from (select datetime(created_at, 'localtime') as local from note)
            group by 1 order by 1 desc
            "#,
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| r.start..r.end)
        .collect())
    }

    /// Return all [`Note`]s which were created in the given date range.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn date_range(&self, range: Range<Date>) -> Result<Vec<Note>, sqlx::Error> {
        let start = OffsetDateTime::new_utc(range.start, Time::MIDNIGHT);
        let end = OffsetDateTime::new_utc(range.end, Time::MIDNIGHT);
        sqlx::query_as!(
            Note,
            r#"
            select note_id as "note_id: Hyphenated", body, created_at as "created_at: OffsetDateTime"
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

/// A shitpost with a Markdown body.
#[derive(Debug)]
pub struct Note {
    /// The note's unique ID.
    pub note_id: Hyphenated,
    // The note's Markdown body.
    pub body: String,
    /// The date and time at which the note was created.
    pub created_at: OffsetDateTime,
}

impl Note {
    /// Returns the note's body as HTML.
    pub fn to_html(&self) -> String {
        let mut out = String::with_capacity(256);
        pulldown_cmark::html::push_html(&mut out, parse_md(&self.body));
        out
    }

    /// Return a vec of the URLs of all images in the note.
    pub fn images(&self, base_url: &Url) -> Vec<Url> {
        parse_md(&self.body)
            .flat_map(|e| match e {
                Event::Start(Tag::Image { dest_url, .. }) => {
                    if dest_url.starts_with("http://") || dest_url.starts_with("https://") {
                        dest_url.parse().ok()
                    } else {
                        base_url.join(dest_url.as_ref()).ok()
                    }
                }
                _ => None,
            })
            .collect()
    }

    /// Returns a plain-text version of the note.
    pub fn description(&self) -> String {
        let mut out = String::with_capacity(256);
        for e in parse_md(&self.body) {
            match e {
                Event::Text(dest_url)
                | Event::Start(Tag::Image { dest_url, .. })
                | Event::Start(Tag::Link { dest_url, .. }) => out.push_str(dest_url.as_ref()),
                Event::SoftBreak
                | Event::HardBreak
                | Event::Start(Tag::Paragraph)
                | Event::Rule => out.push(' '),
                _ => {}
            }
        }
        out.trim().into()
    }
}

fn parse_md(md: &str) -> Parser {
    Parser::new_ext(md, Options::ENABLE_SMART_PUNCTUATION | Options::ENABLE_STRIKETHROUGH)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn body_to_html() {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: r#"It's ~~not~~ _electric_!"#.into(),
            created_at: OffsetDateTime::now_utc(),
        };

        assert_eq!(note.to_html(), "<p>It’s <del>not</del> <em>electric</em>!</p>\n");
    }

    #[test]
    fn body_to_description() {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: "It's _electric_!\n\nBoogie woogie woogie.".into(),
            created_at: OffsetDateTime::now_utc(),
        };

        assert_eq!(note.description(), r#"It’s electric! Boogie woogie woogie."#);
    }
}

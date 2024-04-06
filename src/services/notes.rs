use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};
use rusqlite::{params, OptionalExtension, Row};
use time::{Date, OffsetDateTime, Time};
use tokio_rusqlite::Connection;
use url::Url;

use crate::id::PublicId;

/// A service for creating and viewing [`Note`]s.
#[derive(Debug, Clone)]
pub struct NoteService {
    db: Connection,
}

impl NoteService {
    /// Create a new [`NoteService`] using the given database.
    pub fn new(db: Connection) -> NoteService {
        NoteService { db }
    }

    /// Create a new [`Note`], returning the new note's ID.
    #[must_use]
    #[tracing::instrument(skip(self, body), ret(Display), err)]
    pub async fn create(&self, body: String) -> Result<PublicId, tokio_rusqlite::Error> {
        let note_id = PublicId::random();
        self.db
            .call_unwrap(move |conn| {
                conn.prepare_cached(r#"insert into note (note_id, body) values (?, ?)"#)?
                    .execute(params![note_id, body])
            })
            .await?;
        Ok(note_id)
    }

    /// Find a [`Note`] by ID.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn by_id(&self, note_id: &str) -> Result<Option<Note>, tokio_rusqlite::Error> {
        let note_id = note_id.to_string();
        Ok(self
            .db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    select note_id, body, created_at
                    from note
                    where note_id = ?
                    "#,
                )?
                .query_row(params![note_id], |row| row.try_into())
            })
            .await
            .optional()?)
    }

    /// Find the `n` most recent [`Note`]s in reverse chronological order.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn most_recent(&self, n: u16) -> Result<Vec<Note>, tokio_rusqlite::Error> {
        Ok(self
            .db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    select note_id, body, created_at
                    from note
                    order by created_at desc
                    limit ?
                    "#,
                )?
                .query_map(params![n], |row| row.try_into())?
                .collect::<Result<Vec<_>, _>>()
            })
            .await?)
    }

    /// Return a vec of all week-long date ranges in which notes were created.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn weeks(&self) -> Result<Vec<Range<Date>>, tokio_rusqlite::Error> {
        Ok(self
            .db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    select
                        date(local, 'weekday 0', '-7 days'),
                        date(local, 'weekday 0')
                    from (select datetime(created_at, 'localtime') as local from note)
                    group by 1 order by 1 desc
                    "#,
                )?
                .query_map([], |row| {
                    let start = row.get::<_, Date>(0)?;
                    let end = row.get::<_, Date>(1)?;
                    Ok(start..end)
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .await?)
    }

    /// Return all [`Note`]s which were created in the given date range.
    #[must_use]
    #[tracing::instrument(skip(self), err)]
    pub async fn date_range(&self, range: Range<Date>) -> Result<Vec<Note>, tokio_rusqlite::Error> {
        let start = OffsetDateTime::new_utc(range.start, Time::MIDNIGHT);
        let end = OffsetDateTime::new_utc(range.end, Time::MIDNIGHT);

        Ok(self
            .db
            .call_unwrap(move |conn| {
                conn.prepare_cached(
                    r#"
                    select note_id, body, created_at
                    from note
                    where created_at >= ? and created_at < ?
                    order by created_at desc
                    "#,
                )?
                .query_map(params![start, end], |row| row.try_into())?
                .collect::<Result<Vec<_>, _>>()
            })
            .await?)
    }
}

/// A shitpost with a Markdown body.
#[derive(Debug)]
pub struct Note {
    /// The note's unique ID.
    pub note_id: PublicId,
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

impl<'stmt> TryFrom<&'stmt Row<'stmt>> for Note {
    type Error = rusqlite::Error;

    fn try_from(row: &'stmt Row<'stmt>) -> Result<Self, Self::Error> {
        Ok(Note { note_id: row.get(0)?, body: row.get(1)?, created_at: row.get(2)? })
    }
}

fn parse_md(md: &str) -> Parser {
    Parser::new_ext(md, Options::ENABLE_SMART_PUNCTUATION | Options::ENABLE_STRIKETHROUGH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_to_html() {
        let note = Note {
            note_id: PublicId::random(),
            body: r#"It's ~~not~~ _electric_!"#.into(),
            created_at: OffsetDateTime::now_utc(),
        };

        assert_eq!(note.to_html(), "<p>It’s <del>not</del> <em>electric</em>!</p>\n");
    }

    #[test]
    fn body_to_description() {
        let note = Note {
            note_id: PublicId::random(),
            body: "It's _electric_!\n\nBoogie woogie woogie.".into(),
            created_at: OffsetDateTime::now_utc(),
        };

        assert_eq!(note.description(), r#"It’s electric! Boogie woogie woogie."#);
    }
}

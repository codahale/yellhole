use std::ops::Range;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug)]
pub struct Note {
    pub note_id: String,
    pub body: String,
    pub created_at: NaiveDateTime,
}

impl Note {
    pub async fn create(db: &SqlitePool, note_id: &Uuid, body: &str) -> Result<(), sqlx::Error> {
        let note_id = note_id.to_string();
        sqlx::query!(
            r"
            insert into note (note_id, body) values (?, ?)
            ",
            note_id,
            body
        )
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn by_id(db: &SqlitePool, note_id: &Uuid) -> Result<Option<Note>, sqlx::Error> {
        let note_id = note_id.to_string();
        sqlx::query_as!(
            Note,
            r"
            select note_id, body, created_at
            from note
            where note_id = ?
            ",
            note_id
        )
        .fetch_optional(db)
        .await
    }

    pub async fn most_recent(db: &SqlitePool, n: u16) -> Result<Vec<Note>, sqlx::Error> {
        sqlx::query_as!(
            Note,
            r"
            select note_id, body, created_at
            from note
            order by created_at desc
            limit ?
            ",
            n
        )
        .fetch_all(db)
        .await
    }

    pub async fn date_range(
        db: &SqlitePool,
        range: Range<NaiveDate>,
    ) -> Result<Option<Vec<Note>>, sqlx::Error> {
        let start = local_date_to_utc(&range.start);
        let end = local_date_to_utc(&range.end);
        sqlx::query_as!(
            Note,
            r"
            select note_id, body, created_at
            from note
            where created_at >= ? and created_at < ?
            order by created_at desc
            ",
            start,
            end,
        )
        .fetch_all(db)
        .await
        .map(Some)
    }

    pub fn to_html(&self) -> String {
        render_markdown(&self.body)
    }
}

fn local_date_to_utc(d: &NaiveDate) -> DateTime<Utc> {
    Local.from_local_datetime(&d.and_time(NaiveTime::default())).unwrap().with_timezone(&Utc)
}

fn render_markdown(md: &str) -> String {
    // Downgrade note headings to avoid having multiple H1s.
    fn downgrade_header(level: HeadingLevel) -> Option<HeadingLevel> {
        match level {
            HeadingLevel::H1 => Some(HeadingLevel::H2),
            HeadingLevel::H2 => Some(HeadingLevel::H3),
            HeadingLevel::H3 => Some(HeadingLevel::H4),
            HeadingLevel::H4 => Some(HeadingLevel::H5),
            HeadingLevel::H5 => Some(HeadingLevel::H6),
            HeadingLevel::H6 => None,
        }
    }

    // Parse the note body as Markdown, downgrading headers.
    let parser = Parser::new(md).map(|e| match e {
        Event::Start(Tag::Heading(level, frag, classes)) => match downgrade_header(level) {
            Some(level) => Event::Start(Tag::Heading(level, frag, classes)),
            None => Event::Start(Tag::Strong),
        },
        Event::End(Tag::Heading(level, frag, classes)) => match downgrade_header(level) {
            Some(level) => Event::End(Tag::Heading(level, frag, classes)),
            None => Event::End(Tag::Strong),
        },
        e => e,
    });

    // Render the parsed Markdown AST as HTML.
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, parser);
    out
}

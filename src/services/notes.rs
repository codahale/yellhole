use std::ops::Range;

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use sqlx::SqlitePool;
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
    pub async fn months(&self) -> Result<Vec<NaiveDate>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"
            select strftime('%Y-%m-01', datetime(created_at, 'localtime')) as "month!: NaiveDate"
            from note
            group by 1
            order by 1 desc
            "#
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| r.month)
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    #[test]
    fn render_markdown() {
        let note = Note {
            note_id: Uuid::new_v4().hyphenated(),
            body: r#"
# This is a heading.
## This is a subheading.
### This is a sub-sub-heading.
#### This is a section heading?
##### This is a nitpick.
###### Unclear.
            "#
            .trim()
            .into(),
            created_at: Utc::now(),
        };

        assert_eq!(
            r#"
<h2>This is a heading.</h2>
<h3>This is a subheading.</h3>
<h4>This is a sub-sub-heading.</h4>
<h5>This is a section heading?</h5>
<h6>This is a nitpick.</h6>
<strong>Unclear.</strong>"#
                .trim(),
            note.to_html()
        );
    }
}

use chrono::{Months, NaiveDate, NaiveDateTime, TimeZone, Utc};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use sqlx::SqlitePool;

#[derive(Debug)]
pub struct Note {
    pub note_id: String,
    pub body: String,
    pub created_at: NaiveDateTime,
}

impl Note {
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

    pub async fn month(db: &SqlitePool, year: i32, month: u32) -> Result<Vec<Note>, sqlx::Error> {
        let (start, end) = month_range(year, month);

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
    }

    pub fn to_html(&self) -> String {
        let p = Parser::new(&self.body).map(|e| match e {
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
        let mut html_output = String::new();
        pulldown_cmark::html::push_html(&mut html_output, p);
        html_output
    }
}

fn month_range(year: i32, month: u32) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let end = start + Months::new(1);
    (
        Utc.from_local_date(&start).unwrap().naive_local(),
        Utc.from_local_date(&end).unwrap().naive_local(),
    )
}

#[derive(Debug)]
pub struct Link {
    pub link_id: String,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
}

impl Link {
    pub async fn most_recent(db: &SqlitePool, n: u16) -> Result<Vec<Link>, sqlx::Error> {
        sqlx::query_as!(
            Link,
            r"
            select link_id, title, url, description, created_at
            from link
            order by created_at desc
            limit ?
            ",
            n
        )
        .fetch_all(db)
        .await
    }

    pub async fn month(db: &SqlitePool, year: i32, month: u32) -> Result<Vec<Link>, sqlx::Error> {
        let (start, end) = month_range(year, month);

        sqlx::query_as!(
            Link,
            r"
            select link_id, title, url, description, created_at
            from link
            where created_at >= ? and created_at < ?
            order by created_at desc
            ",
            start,
            end,
        )
        .fetch_all(db)
        .await
    }
}

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

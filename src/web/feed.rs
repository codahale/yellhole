use askama::Template;
use atom_syndication::{Content, Entry, Feed, FixedDateTime, Link, Person, Text};
use axum::extract::{Path, Query};
use axum::http::{self, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use chrono::{Datelike, FixedOffset, Months, NaiveDate, Utc};
use serde::Deserialize;
use tower_http::set_header::SetResponseHeaderLayer;
use url::Url;
use uuid::Uuid;

use super::Page;
use crate::config::{Author, Title};
use crate::services::notes::{Note, NoteService};

pub fn router() -> Router {
    let immutable = Router::new().route("/note/:note_id", get(single)).layer(
        SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=31536000,immutable"),
        ),
    );

    Router::new()
        .route("/", get(index))
        .route("/atom.xml", get(atom))
        .route("/notes/:year/:month", get(month))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=300"),
        ))
        .merge(immutable)
}

#[derive(Debug, Template)]
#[template(path = "feed.html")]
struct FeedPage {
    notes: Vec<Note>,
    base_url: Url,
    newer: Option<NaiveDate>,
    older: Option<NaiveDate>,
}

mod filters {
    use chrono::{DateTime, Local, NaiveDateTime, TimeZone};

    pub fn to_local_tz(t: &NaiveDateTime) -> askama::Result<DateTime<Local>> {
        Ok(Local.from_utc_datetime(t))
    }
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(
    notes: Extension<NoteService>,
    Extension(base_url): Extension<Url>,
    opts: Query<IndexOpts>,
) -> Result<Page<FeedPage>, StatusCode> {
    let n = opts.n.unwrap_or(100);
    let notes = notes.most_recent(n).await.map_err(|err| {
        tracing::warn!(?err, n, "error querying feed index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let older = notes.last().and_then(|n| n.created_at.date().with_day(1));

    Ok(Page(FeedPage { notes, base_url, newer: None, older }))
}

async fn atom(
    notes: Extension<NoteService>,
    base_url: Extension<Url>,
    Extension(Author(author)): Extension<Author>,
    Extension(Title(title)): Extension<Title>,
) -> Result<Response, StatusCode> {
    let notes = notes.most_recent(20).await.map_err(|err| {
        tracing::warn!(?err, "error querying atom index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries = notes
        .iter()
        .map(|n| Entry {
            id: base_url.join(&format!("note/{}", n.note_id)).unwrap().to_string(),
            title: Text { value: n.note_id.to_string(), ..Default::default() },
            content: Some(Content {
                content_type: Some("html".into()),
                value: Some(n.to_html()),
                ..Default::default()
            }),
            updated: FixedDateTime::from_local(n.created_at, FixedOffset::east_opt(0).unwrap()),
            ..Default::default()
        })
        .collect();

    let feed = Feed {
        id: base_url.to_string(),
        authors: vec![Person { name: author, ..Default::default() }],
        base: Some(base_url.to_string()),
        title: Text { value: title, ..Default::default() },
        entries,
        links: vec![Link { href: base_url.to_string(), rel: "self".into(), ..Default::default() }],
        updated: FixedDateTime::from_utc(Utc::now().naive_utc(), FixedOffset::east_opt(0).unwrap()),
        ..Default::default()
    };

    Ok((
        [(http::header::CONTENT_TYPE, http::HeaderValue::from_static(mime::TEXT_XML.as_ref()))],
        feed.to_string(),
    )
        .into_response())
}

async fn month(
    notes: Extension<NoteService>,
    Extension(base_url): Extension<Url>,
    Path((year, month)): Path<(i32, u32)>,
) -> Result<Page<FeedPage>, StatusCode> {
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else { return Err(StatusCode::NOT_FOUND)};
    let end = start + Months::new(1);

    let notes = notes
        .date_range(start..end)
        .await
        .map_err(|err| {
            tracing::warn!(?err, year, month, "error querying feed for month");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes, base_url, newer: Some(end), older: Some(start - Months::new(1)) }))
}

async fn single(
    notes: Extension<NoteService>,
    Extension(base_url): Extension<Url>,
    Path(note_id): Path<String>,
) -> Result<Page<FeedPage>, StatusCode> {
    let note_id = note_id.parse::<Uuid>().map_err(|_| StatusCode::NOT_FOUND)?;
    let note = notes
        .by_id(note_id.as_hyphenated())
        .await
        .map_err(|err| {
            tracing::warn!(?err, %note_id, "error querying feed by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes: vec![note], base_url, newer: None, older: None }))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::test_server::TestServer;

    use super::*;

    use sqlx::SqlitePool;

    #[sqlx::test(fixtures("notes"))]
    async fn main(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn atom_feed(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/atom.xml").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/xml"))
        );

        let feed = Feed::read_from(Cursor::new(&resp.bytes().await?))?;
        assert_eq!(
            feed.entries[0].content().unwrap().value().unwrap(),
            "<p>It's a me, <em>Mario</em>.</p>\n"
        );

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn monthly_view(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/notes/2022/10").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn single_note(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/note/c1449d6c-6b5b-4ce4-a4d7-98853562fbf1").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn bad_note_id(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/note/not-a-uuid").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn missing_note_id(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(&db))?;

        let resp = ts.get("/note/37c615b0-bb55-424d-a813-69e14ca5c20c").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    fn app(db: &SqlitePool) -> Router {
        router()
            .layer(Extension(NoteService::new(db.clone())))
            .layer(Extension("http://example.com".parse::<Url>().unwrap()))
            .layer(Extension(Author("Mr Magoo".into())))
            .layer(Extension(Title("Yellhole".into())))
    }
}

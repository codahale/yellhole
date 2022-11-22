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
use uuid::Uuid;

use super::Page;
use crate::config::Config;
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
    config: Config,
    notes: Vec<Note>,
    months: Vec<NaiveDate>,
}

mod filters {
    use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};

    pub fn to_local_tz(t: &DateTime<Utc>) -> askama::Result<DateTime<Local>> {
        Ok(t.with_timezone(&Local))
    }

    pub fn to_month(d: &NaiveDate) -> askama::Result<String> {
        Ok(format!("{:04}/{:02}", d.year(), d.month()))
    }
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(
    notes: Extension<NoteService>,
    Extension(config): Extension<Config>,
    opts: Query<IndexOpts>,
) -> Result<Page<FeedPage>, StatusCode> {
    let months = notes.months().await.map_err(|err| {
        tracing::warn!(?err, "error querying note dates");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let n = opts.n.unwrap_or(100);
    let notes = notes.most_recent(n).await.map_err(|err| {
        tracing::warn!(?err, n, "error querying feed index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Page(FeedPage { config, notes, months }))
}

async fn atom(
    notes: Extension<NoteService>,
    config: Extension<Config>,
) -> Result<Response, StatusCode> {
    let notes = notes.most_recent(20).await.map_err(|err| {
        tracing::warn!(?err, "error querying atom index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries = notes
        .iter()
        .map(|n| Entry {
            id: config.base_url.join(&format!("note/{}", n.note_id)).unwrap().to_string(),
            title: Text { value: n.note_id.to_string(), ..Default::default() },
            content: Some(Content {
                content_type: Some("html".into()),
                value: Some(n.to_html()),
                ..Default::default()
            }),
            updated: n.created_at.with_timezone(&FixedOffset::east_opt(0).unwrap()),
            ..Default::default()
        })
        .collect();

    let feed = Feed {
        id: config.base_url.to_string(),
        authors: vec![Person { name: config.author.clone(), ..Default::default() }],
        base: Some(config.base_url.join("atom.xml").unwrap().to_string()),
        title: Text { value: config.title.clone(), ..Default::default() },
        entries,
        links: vec![Link {
            href: config.base_url.join("atom.xml").unwrap().to_string(),
            rel: "self".into(),
            ..Default::default()
        }],
        updated: FixedDateTime::from_utc(Utc::now().naive_utc(), FixedOffset::east_opt(0).unwrap()),
        ..Default::default()
    };

    Ok((
        [(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/atom+xml; charset=utf-8"),
        )],
        feed.to_string(),
    )
        .into_response())
}

async fn month(
    notes: Extension<NoteService>,
    Extension(config): Extension<Config>,
    Path((year, month)): Path<(i32, u32)>,
) -> Result<Page<FeedPage>, StatusCode> {
    let months = notes.months().await.map_err(|err| {
        tracing::warn!(?err, "error querying note dates");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or(StatusCode::NOT_FOUND)?;
    let end = start + Months::new(1);

    let notes = notes.date_range(start..end).await.map_err(|err| {
        tracing::warn!(?err, year, month, "error querying feed for month");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Page(FeedPage { config, notes, months }))
}

async fn single(
    notes: Extension<NoteService>,
    Extension(config): Extension<Config>,
    Path(note_id): Path<String>,
) -> Result<Page<FeedPage>, StatusCode> {
    let months = notes.months().await.map_err(|err| {
        tracing::warn!(?err, "error querying note dates");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let note_id = note_id.parse::<Uuid>().map_err(|_| StatusCode::NOT_FOUND)?;
    let note = notes
        .by_id(&note_id)
        .await
        .map_err(|err| {
            tracing::warn!(?err, %note_id, "error querying feed by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { config, notes: vec![note], months }))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use sqlx::SqlitePool;
    use url::Url;

    use crate::test_server::TestServer;

    use super::*;

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
            Some(&http::HeaderValue::from_static("application/atom+xml; charset=utf-8"))
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
        router().layer(Extension(NoteService::new(db.clone()))).layer(Extension(Config {
            port: 8080,
            base_url: "http://example.com".parse::<Url>().unwrap(),
            data_dir: ".".into(),
            title: "Yellhole".into(),
            author: "Luther Blissett".into(),
        }))
    }
}

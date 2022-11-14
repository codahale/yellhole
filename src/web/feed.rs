use askama::Template;
use atom_syndication::{Content, Entry, Feed, FixedDateTime, Link, Person, Text};
use axum::extract::{Path, Query};
use axum::http;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use chrono::{Datelike, FixedOffset, Months, NaiveDate, Utc};
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_http::set_header::SetResponseHeaderLayer;
use url::Url;
use uuid::Uuid;

use super::Page;
use crate::config::{Author, Title};
use crate::models::Note;

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
    db: Extension<SqlitePool>,
    Extension(base_url): Extension<Url>,
    opts: Query<IndexOpts>,
) -> Result<Page<FeedPage>, StatusCode> {
    let n = opts.n.unwrap_or(100);
    let notes = Note::most_recent(&db, n).await.map_err(|err| {
        tracing::warn!(?err, n, "error querying feed index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let older = notes.last().and_then(|n| n.created_at.date().with_day(1));

    Ok(Page(FeedPage { notes, base_url, newer: None, older }))
}

async fn atom(
    db: Extension<SqlitePool>,
    base_url: Extension<Url>,
    Extension(Author(author)): Extension<Author>,
    Extension(Title(title)): Extension<Title>,
) -> Result<Response, StatusCode> {
    let notes = Note::most_recent(&db, 20).await.map_err(|err| {
        tracing::warn!(?err, "error querying atom index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries = notes
        .iter()
        .map(|n| Entry {
            id: base_url.join(&format!("note/{}", n.note_id)).unwrap().to_string(),
            title: Text { value: n.note_id.clone(), ..Default::default() },
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
    db: Extension<SqlitePool>,
    Extension(base_url): Extension<Url>,
    Path((year, month)): Path<(i32, u32)>,
) -> Result<Page<FeedPage>, StatusCode> {
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else { return Err(StatusCode::NOT_FOUND)};
    let end = start + Months::new(1);

    let notes = Note::date_range(&db, start..end)
        .await
        .map_err(|err| {
            tracing::warn!(?err, year, month, "error querying feed for month");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes, base_url, newer: Some(end), older: Some(start - Months::new(1)) }))
}

async fn single(
    db: Extension<SqlitePool>,
    Extension(base_url): Extension<Url>,
    Path(note_id): Path<Option<Uuid>>,
) -> Result<Page<FeedPage>, StatusCode> {
    let note_id = note_id.ok_or(StatusCode::NOT_FOUND)?;
    let note = Note::by_id(&db, &note_id)
        .await
        .map_err(|err| {
            tracing::warn!(?err, %note_id, "error querying feed by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes: vec![note], base_url, newer: None, older: None }))
}

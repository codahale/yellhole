use askama::Template;
use atom_syndication::{Content, Entry, Feed, FixedDateTime, Link, Person, Text};
use axum::extract::{Host, Path, Query};
use axum::http;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use chrono::{Datelike, FixedOffset, Months, NaiveDate, Utc};
use serde::Deserialize;
use tower_http::set_header::SetResponseHeaderLayer;

use super::{Context, Page};
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
    host: String,
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
    ctx: Extension<Context>,
    opts: Query<IndexOpts>,
    Host(host): Host,
) -> Result<Page<FeedPage>, StatusCode> {
    let n = opts.n.unwrap_or(100);
    let notes = Note::most_recent(&ctx.db, n).await.map_err(|err| {
        tracing::warn!(?err, n, "error querying feed index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let older = notes.last().and_then(|n| n.created_at.date().with_day(1));

    Ok(Page(FeedPage { notes, host, newer: None, older }))
}

async fn atom(ctx: Extension<Context>, Host(host): Host) -> Result<Response, StatusCode> {
    let notes = Note::most_recent(&ctx.db, 20).await.map_err(|err| {
        tracing::warn!(?err, "error querying atom index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries = notes
        .iter()
        .map(|n| Entry {
            id: format!("https://{host}/note/{}", n.note_id),
            title: Text { value: n.note_id.clone(), ..Default::default() },
            content: Some(Content {
                content_type: Some("html".into()),
                value: Some(n.to_html()),
                ..Default::default()
            }),
            updated: FixedDateTime::from_local(n.created_at, FixedOffset::east(0)),
            ..Default::default()
        })
        .collect();

    let feed = Feed {
        id: format!("https://{host}/"),
        authors: vec![Person { name: ctx.author.clone(), ..Default::default() }],
        base: Some(format!("https://{host}")),
        title: Text { value: ctx.name.clone(), ..Default::default() },
        entries,
        links: vec![Link {
            href: format!("https://{host}"),
            rel: "self".into(),
            ..Default::default()
        }],
        updated: FixedDateTime::from_utc(Utc::now().naive_utc(), FixedOffset::east(0)),
        ..Default::default()
    };

    Ok((
        [(http::header::CONTENT_TYPE, http::HeaderValue::from_static(mime::TEXT_XML.as_ref()))],
        feed.to_string(),
    )
        .into_response())
}

async fn month(
    ctx: Extension<Context>,
    Path((year, month)): Path<(i32, u32)>,
    Host(host): Host,
) -> Result<Page<FeedPage>, StatusCode> {
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else { return Err(StatusCode::NOT_FOUND)};
    let end = start + Months::new(1);

    let notes = Note::date_range(&ctx.db, start..end)
        .await
        .map_err(|err| {
            tracing::warn!(?err, year, month, "error querying feed for month");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes, host, newer: Some(end), older: Some(start - Months::new(1)) }))
}

async fn single(
    ctx: Extension<Context>,
    Path(note_id): Path<String>,
    Host(host): Host,
) -> Result<Page<FeedPage>, StatusCode> {
    let note = Note::by_id(&ctx.db, &note_id)
        .await
        .map_err(|err| {
            tracing::warn!(?err, note_id, "error querying feed by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Page(FeedPage { notes: vec![note], host, newer: None, older: None }))
}

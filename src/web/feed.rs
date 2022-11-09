use std::time::Duration;

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

use super::{CacheControl, Context, Html};
use crate::models::Note;

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/atom.xml", get(atom))
        .route("/notes/:year/:month", get(month))
        .route("/note/:note_id", get(single))
}

#[derive(Debug, Template)]
#[template(path = "feed.html")]
struct FeedPage {
    notes: Vec<Note>,
    newer: Option<NaiveDate>,
    older: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(
    ctx: Extension<Context>,
    opts: Query<IndexOpts>,
) -> Result<Html<FeedPage>, StatusCode> {
    let n = opts.n.unwrap_or(100);
    let notes = Note::most_recent(&ctx.db, n).await.map_err(|err| {
        tracing::warn!(?err, n, "error querying feed index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let older = notes.last().and_then(|n| n.created_at.date().with_day(1));

    Ok(Html(FeedPage { notes, newer: None, older }, DEFAULT_CACHING))
}

async fn atom(ctx: Extension<Context>) -> Result<Response, StatusCode> {
    let notes = Note::most_recent(&ctx.db, 20).await.map_err(|err| {
        tracing::warn!(?err, "error querying atom index");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let entries = notes
        .iter()
        .map(|n| Entry {
            id: ctx
                .base_url
                .join(&format!("/note/{}", n.note_id))
                .expect("invalid URl")
                .to_string(),
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
        id: ctx.base_url.to_string(),
        authors: vec![Person { name: ctx.author.clone(), ..Default::default() }],
        base: Some(ctx.base_url.to_string()),
        title: Text { value: ctx.name.clone(), ..Default::default() },
        entries,
        links: vec![Link {
            href: ctx.base_url.to_string(),
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
) -> Result<Html<FeedPage>, StatusCode> {
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else { return Err(StatusCode::NOT_FOUND)};
    let end = start + Months::new(1);

    let notes = Note::date_range(&ctx.db, start..end)
        .await
        .map_err(|err| {
            tracing::warn!(?err, year, month, "error querying feed for month");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(
        FeedPage { notes, newer: Some(end), older: Some(start - Months::new(1)) },
        DEFAULT_CACHING,
    ))
}

async fn single(
    ctx: Extension<Context>,
    Path(note_id): Path<String>,
) -> Result<Html<FeedPage>, StatusCode> {
    let note = Note::by_id(&ctx.db, &note_id)
        .await
        .map_err(|err| {
            tracing::warn!(?err, note_id, "error querying feed by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(FeedPage { notes: vec![note], newer: None, older: None }, CacheControl::Immutable))
}

const DEFAULT_CACHING: CacheControl = CacheControl::MaxAge(Duration::from_secs(60 * 5));

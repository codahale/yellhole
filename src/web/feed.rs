use std::time::Duration;

use askama::Template;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Router};
use serde::Deserialize;

use super::{CacheControl, Context, Html};
use crate::models::Note;

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/notes/:year/:month", get(month))
        .route("/note/:note_id", get(single))
}

#[derive(Debug, Template)]
#[template(path = "feed.html")]
struct FeedPage {
    notes: Vec<Note>,
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(
    ctx: Extension<Context>,
    opts: Query<IndexOpts>,
) -> Result<Html<FeedPage>, StatusCode> {
    let notes = Note::most_recent(&ctx.db, opts.n.unwrap_or(100)).await.map_err(|e| {
        log::warn!("error querying feed index: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Html(FeedPage { notes }, DEFAULT_CACHING))
}

async fn month(
    ctx: Extension<Context>,
    Path((year, month)): Path<(i32, u32)>,
) -> Result<Html<FeedPage>, StatusCode> {
    let notes = Note::month(&ctx.db, year, month)
        .await
        .map_err(|e| {
            log::warn!("error querying feed for month `{year}/{month}`: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(FeedPage { notes }, DEFAULT_CACHING))
}

async fn single(
    ctx: Extension<Context>,
    Path(note_id): Path<String>,
) -> Result<Html<FeedPage>, StatusCode> {
    let note = Note::by_id(&ctx.db, &note_id)
        .await
        .map_err(|e| {
            log::warn!("error querying feed for note `{note_id}`: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(FeedPage { notes: vec![note] }, CacheControl::Immutable))
}

const DEFAULT_CACHING: CacheControl = CacheControl::MaxAge(Duration::from_secs(60 * 5));

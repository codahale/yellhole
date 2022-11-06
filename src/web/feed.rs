use askama::Template;
use axum::extract::{Path, Query};
use axum::routing::get;
use axum::{Extension, Router};
use serde::Deserialize;

use super::{Context, Html, WebError};
use crate::models::Note;

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/notes/:year/:month", get(month))
        .route("/note/:note_id", get(single))
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(ctx: Extension<Context>, opts: Query<IndexOpts>) -> Result<Html<Index>, WebError> {
    let notes = Note::most_recent(&ctx.db, opts.n.unwrap_or(100)).await?;

    Ok(Html(Index { notes }))
}

async fn month(
    ctx: Extension<Context>,
    Path((year, month)): Path<(i32, u32)>,
) -> Result<Html<Index>, WebError> {
    let notes = Note::month(&ctx.db, year, month).await?.ok_or(WebError::NotFound)?;
    Ok(Html(Index { notes }))
}

async fn single(
    ctx: Extension<Context>,
    Path(note_id): Path<String>,
) -> Result<Html<Index>, WebError> {
    let note = Note::by_id(&ctx.db, &note_id).await?.ok_or(WebError::NotFound)?;
    Ok(Html(Index { notes: vec![note] }))
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    notes: Vec<Note>,
}

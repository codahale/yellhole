use askama::Template;
use axum::extract::{Path, Query};
use axum::routing::get;
use axum::{Extension, Router};
use serde::Deserialize;

use super::{Context, Html};
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

async fn index(ctx: Extension<Context>, opts: Query<IndexOpts>) -> Html<Index> {
    let notes = Note::most_recent(&ctx.db, opts.n.unwrap_or(100)).await.expect("whoops");

    Html(Index { notes })
}

async fn month(ctx: Extension<Context>, Path((year, month)): Path<(i32, u32)>) -> Html<Index> {
    let notes = Note::month(&ctx.db, year, month).await.expect("whoops");

    Html(Index { notes })
}

async fn single(ctx: Extension<Context>, Path(note_id): Path<String>) -> Html<Index> {
    let note = Note::by_id(&ctx.db, &note_id).await.expect("whoops").expect("not found");

    Html(Index { notes: vec![note] })
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    notes: Vec<Note>,
}

use askama::Template;
use axum::extract::Path;
use axum::routing::get;
use axum::{Extension, Router};

use crate::models::Note;

use super::{Context, Html};

pub fn router() -> Router {
    Router::new().route("/", get(index)).route("/feed/:year/:month", get(month))
}

async fn index(ctx: Extension<Context>) -> Html<Index> {
    let notes = Note::most_recent(&ctx.db, 100).await.expect("whoops");

    Html(Index { notes })
}

async fn month(ctx: Extension<Context>, Path((year, month)): Path<(i32, u32)>) -> Html<Index> {
    let notes = Note::month(&ctx.db, year, month).await.expect("whoops");

    Html(Index { notes })
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    notes: Vec<Note>,
}

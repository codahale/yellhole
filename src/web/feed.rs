use askama::Template;
use axum::routing::get;
use axum::{Extension, Router};

use crate::models::Note;

use super::{Context, Html};

pub fn router() -> Router {
    Router::new().route("/", get(index))
}

async fn index(ctx: Extension<Context>) -> Html<Index> {
    let notes = Note::most_recent(&ctx.db, 100).await.expect("whoops");

    Html(Index { notes })
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    notes: Vec<Note>,
}

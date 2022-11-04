use askama::Template;
use axum::extract::Path;
use axum::routing::get;
use axum::{Extension, Router};
use chrono::NaiveDateTime;

use crate::models::{Link, Note};

use super::{Context, Html};

pub fn router() -> Router {
    Router::new().route("/", get(index)).route("/feed/:year/:month", get(month))
}

async fn index(ctx: Extension<Context>) -> Html<Index> {
    let notes = Note::most_recent(&ctx.db, 100).await.expect("whoops");
    let links = Link::most_recent(&ctx.db, 100).await.expect("whoops");
    let feed = Content::from_parts(notes, links, Some(100));

    Html(Index { feed })
}

async fn month(ctx: Extension<Context>, Path((year, month)): Path<(i32, u32)>) -> Html<Index> {
    let notes = Note::month(&ctx.db, year, month).await.expect("whoops");
    let links = Link::month(&ctx.db, year, month).await.expect("whoops");
    let feed = Content::from_parts(notes, links, None);

    Html(Index { feed })
}

#[derive(Debug)]
enum Content {
    Note(Note),
    Link(Link),
}

impl Content {
    pub fn from_parts(notes: Vec<Note>, links: Vec<Link>, n: Option<usize>) -> Vec<Content> {
        let mut feed = notes
            .into_iter()
            .map(Content::Note)
            .chain(links.into_iter().map(Content::Link))
            .collect::<Vec<Content>>();

        feed.sort_by_key(|c| -c.created_at().timestamp());
        if let Some(n) = n {
            feed.truncate(n);
        }

        feed
    }

    pub fn created_at(&self) -> NaiveDateTime {
        match self {
            Content::Note(n) => n.created_at,
            Content::Link(l) => l.created_at,
        }
    }
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    feed: Vec<Content>,
}

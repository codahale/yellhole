use std::net::SocketAddr;

use askama::Template;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{http, Extension, Router};
use chrono::NaiveDateTime;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use sqlx::{FromRow, SqlitePool};
use tower::ServiceBuilder;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::trace::TraceLayer;

pub async fn serve(addr: &SocketAddr, db: SqlitePool) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .layer(AddExtensionLayer::new(Context { db }))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    log::info!("listening on http://{}", addr);
    axum::Server::bind(addr).serve(app.into_make_service()).await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct Context {
    db: SqlitePool,
}

#[derive(FromRow, Debug)]
pub struct Note {
    pub note_id: String,
    pub body: String,
    pub created_at: NaiveDateTime,
}

impl Note {
    pub fn to_html(&self) -> String {
        let p = Parser::new(&self.body).map(|e| match e {
            Event::Start(Tag::Heading(level, frag, classes)) => match level {
                HeadingLevel::H1 => Event::Start(Tag::Heading(HeadingLevel::H2, frag, classes)),
                HeadingLevel::H2 => Event::Start(Tag::Heading(HeadingLevel::H3, frag, classes)),
                HeadingLevel::H3 => Event::Start(Tag::Heading(HeadingLevel::H4, frag, classes)),
                HeadingLevel::H4 => Event::Start(Tag::Heading(HeadingLevel::H5, frag, classes)),
                HeadingLevel::H5 => Event::Start(Tag::Heading(HeadingLevel::H6, frag, classes)),
                HeadingLevel::H6 => Event::Start(Tag::Strong),
            },
            Event::End(Tag::Heading(level, frag, classes)) => match level {
                HeadingLevel::H1 => Event::End(Tag::Heading(HeadingLevel::H2, frag, classes)),
                HeadingLevel::H2 => Event::End(Tag::Heading(HeadingLevel::H3, frag, classes)),
                HeadingLevel::H3 => Event::End(Tag::Heading(HeadingLevel::H4, frag, classes)),
                HeadingLevel::H4 => Event::End(Tag::Heading(HeadingLevel::H5, frag, classes)),
                HeadingLevel::H5 => Event::End(Tag::Heading(HeadingLevel::H6, frag, classes)),
                HeadingLevel::H6 => Event::End(Tag::Strong),
            },
            e => e,
        });
        let mut html_output = String::new();
        pulldown_cmark::html::push_html(&mut html_output, p);
        html_output
    }
}

async fn index(ctx: Extension<Context>) -> Html<Index> {
    let notes = sqlx::query_as!(
        Note,
        r"select note_id, body, created_at from note order by created_at desc limit 50"
    )
    .fetch_all(&ctx.db)
    .await
    .expect("whoops");

    Html(Index { notes })
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Index {
    notes: Vec<Note>,
}

#[derive(Debug)]
struct Html<T: Template>(T);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(body) => {
                let headers =
                    [(http::header::CONTENT_TYPE, http::HeaderValue::from_static(T::MIME_TYPE))];

                (headers, body).into_response()
            }
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

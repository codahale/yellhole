use askama::Template;
use axum::body::BoxBody;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Extension, Form, Router};
use serde::Deserialize;

use crate::models::Note;

use super::{Context, Html, WebError};

pub fn router() -> Router {
    // TODO add authentication
    Router::new().route("/admin/new", get(new_page)).route("/admin/new-note", post(create_note))
}

async fn new_page() -> Html<NewPage> {
    Html(NewPage {})
}

#[derive(Debug, Deserialize)]
struct NewNote {
    body: String,
}

async fn create_note(
    ctx: Extension<Context>,
    Form(new_note): Form<NewNote>,
) -> Result<Response, WebError> {
    let id = Note::create(&ctx.db, &new_note.body).await?;
    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header("location", format!("/note/{id}"))
        .body(BoxBody::default())
        .unwrap())
}

#[derive(Debug, Template)]
#[template(path = "new.html")]
struct NewPage {}

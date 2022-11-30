use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::web::AppState;

#[derive(Debug)]
pub struct Page<T: Template>(pub T);

impl<T: Template> IntoResponse for Page<T> {
    fn into_response(self) -> Response {
        Html(self.0.render().expect("error rendering template")).into_response()
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    status: StatusCode,
}

impl ErrorPage {
    pub fn for_status(status: StatusCode) -> Response {
        (status, Page(ErrorPage { status })).into_response()
    }
}

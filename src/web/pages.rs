use askama::Template;
use axum::http::{self, StatusCode};
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug)]
pub struct Page<T: Template>(pub T);

impl<T: Template> IntoResponse for Page<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(body) => Html(body).into_response(),
            Err(err) => {
                tracing::error!(?err, "unable to render template");
                http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    status: StatusCode,
}

impl ErrorPage {
    pub fn for_status(status: StatusCode) -> Response {
        let mut resp = Page(ErrorPage { status }).into_response();
        *resp.status_mut() = status;
        resp
    }
}

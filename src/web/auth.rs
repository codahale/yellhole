use askama::Template;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

pub fn router() -> Router {
    Router::new()
        .route("/register", get(register_page).post(register))
        .route("/login", get(login_page).post(login))
}

#[derive(Debug, Template)]
#[template(path = "register.html")]
struct RegisterPage {}

async fn register_page() -> Result<(), StatusCode> {
    todo!()
}

async fn register() -> Result<(), StatusCode> {
    todo!()
}

#[derive(Debug, Template)]
#[template(path = "login.html")]
struct LoginPage {}

async fn login_page() -> Result<(), StatusCode> {
    todo!()
}

async fn login() -> Result<(), StatusCode> {
    todo!()
}

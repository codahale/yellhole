use askama::Template;
use axum::extract::{FromRequest, RequestParts};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_sessions::extractors::{ReadableSession, WritableSession};
use sqlx::SqlitePool;
use url::Url;
use uuid::Uuid;

use super::Page;
use crate::config::Author;
use crate::models::passkey::{
    self, AuthenticationChallenge, AuthenticationResponse, RegistrationChallenge,
    RegistrationResponse,
};

pub fn router() -> Router {
    Router::new()
        .route("/register", get(register))
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        .route("/login", get(login))
        .route("/login/start", post(login_start))
        .route("/login/finish", post(login_finish))
}

pub struct RequireAuth;

#[axum::async_trait]
impl<B> FromRequest<B> for RequireAuth
where
    B: Send,
{
    type Rejection = Redirect;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let session = ReadableSession::from_request(req).await.expect("infallible");
        session
            .get::<bool>("authenticated")
            .unwrap_or(false)
            .then_some(Self)
            .ok_or_else(|| Redirect::to("/login"))
    }
}

#[derive(Debug, Template)]
#[template(path = "register.html")]
struct RegisterPage {}

async fn register(
    db: Extension<SqlitePool>,
    session: ReadableSession,
) -> Result<Response, StatusCode> {
    let registered = passkey::any_registered(&db).await.map_err(|err| {
        tracing::warn!(%err, "unable to query DB");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if registered && !session.get::<bool>("authenticated").unwrap_or(false) {
        return Ok(Redirect::to("/login").into_response());
    }

    Ok(Page(RegisterPage {}).into_response())
}

async fn register_start(
    db: Extension<SqlitePool>,
    base_url: Extension<Url>,
    Extension(Author(author)): Extension<Author>,
) -> Result<Json<RegistrationChallenge>, StatusCode> {
    passkey::start_registration(
        &db,
        &base_url,
        &author,
        Uuid::default().as_hyphenated().to_string().as_bytes(),
    )
    .await
    .map(Json)
    .map_err(|err| {
        tracing::warn!(%err, "unable to start passkey registration");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn register_finish(
    db: Extension<SqlitePool>,
    base_url: Extension<Url>,
    Json(resp): Json<RegistrationResponse>,
) -> Result<Response, StatusCode> {
    passkey::finish_registration(&db, &base_url, resp).await.map_err(|err| {
        tracing::warn!(%err, "unable to finish passkey registration");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::CREATED.into_response())
}

#[derive(Debug, Template)]
#[template(path = "login.html")]
struct LoginPage {}

async fn login(
    db: Extension<SqlitePool>,
    session: ReadableSession,
) -> Result<Response, StatusCode> {
    if session.get::<bool>("authenticated").unwrap_or(false) {
        return Ok(Redirect::to("/admin/new").into_response());
    }

    let registered = passkey::any_registered(&db).await.map_err(|err| {
        tracing::warn!(%err, "unable to query DB");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !registered {
        return Ok(Redirect::to("/register").into_response());
    }

    Ok(Page(LoginPage {}).into_response())
}

async fn login_start(
    db: Extension<SqlitePool>,
    base_url: Extension<Url>,
    mut session: WritableSession,
) -> Result<Json<AuthenticationChallenge>, StatusCode> {
    let resp = passkey::start_authentication(&db, &base_url).await.map_err(|err| {
        tracing::warn!(%err, "unable to start passkey authentication");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Store the authentication state in the session.
    session.remove("challenge");
    session.insert("challenge", resp.challenge).map_err(|err| {
        tracing::warn!(%err, "unable to store passkey authentication challenge in session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(resp))
}

async fn login_finish(
    db: Extension<SqlitePool>,
    base_url: Extension<Url>,
    mut session: WritableSession,
    Json(auth): Json<AuthenticationResponse>,
) -> Result<Response, StatusCode> {
    // webauthn_rs::prelude::Webauthn::finish_passkey_authentication(&self, reg, state);
    let challenge = session.get::<[u8; 32]>("challenge").ok_or_else(|| {
        tracing::warn!("no stored authentication state");
        StatusCode::BAD_REQUEST
    })?;
    session.remove("challenge");

    let authenticated =
        passkey::finish_authentication(&db, &base_url, auth, challenge).await.map_err(|err| {
            tracing::warn!(%err, "unable to finish passkey authentication");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if authenticated {
        session.insert("authenticated", true).map_err(|err| {
            tracing::warn!(%err, "unable to store authentication state in session");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        Ok(StatusCode::ACCEPTED.into_response())
    } else {
        Ok(StatusCode::BAD_REQUEST.into_response())
    }
}

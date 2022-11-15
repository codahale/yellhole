use std::sync::Arc;

use askama::Template;
use axum::extract::{FromRequest, RequestParts};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_sessions::extractors::{ReadableSession, WritableSession};
use sqlx::SqlitePool;
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, RequestChallengeResponse,
};
use webauthn_rs::Webauthn;

use super::Page;
use crate::config::Author;
use crate::models::Credential;

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
    let passkeys = Credential::passkeys(&db).await.map_err(|err| {
        tracing::warn!(%err, "unable to select passkeys");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !passkeys.is_empty() && !session.get::<bool>("authenticated").unwrap_or(false) {
        return Ok(Redirect::to("/login").into_response());
    }

    Ok(Page(RegisterPage {}).into_response())
}

async fn register_start(
    db: Extension<SqlitePool>,
    Extension(Author(author)): Extension<Author>,
    webauthn: Extension<Arc<Webauthn>>,
    mut session: WritableSession,
) -> Result<Json<CreationChallengeResponse>, StatusCode> {
    let existing_credentials = Credential::passkeys(&db)
        .await
        .map_err(|err| {
            tracing::warn!(%err, "unable to select passkeys");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .into_iter()
        .map(|p| p.cred_id().clone())
        .collect();

    // Create a registration challenge.
    let (challenge, state) = webauthn
        .start_passkey_registration(Uuid::default(), &author, &author, Some(existing_credentials))
        .map_err(|err| {
            tracing::warn!(%err, "unable to start passkey registration");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Store the registration state in the session.
    session.remove("reg_state");
    session.insert("reg_state", &state).map_err(|err| {
        tracing::warn!(%err, "unable to store passkey registration state in session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(challenge))
}

async fn register_finish(
    db: Extension<SqlitePool>,
    webauthn: Extension<Arc<Webauthn>>,
    mut session: WritableSession,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Result<Response, StatusCode> {
    let state = session.get::<PasskeyRegistration>("reg_state");
    session.remove("reg_state");

    let state = state.ok_or_else(|| {
        tracing::warn!("no stored registration state");
        StatusCode::BAD_REQUEST
    })?;

    let passkey = webauthn.finish_passkey_registration(&reg, &state).map_err(|err| {
        tracing::warn!(?err, "unable to finish passkey registration");
        StatusCode::BAD_REQUEST
    })?;

    Credential::create(&db, passkey).await.map_err(|err| {
        tracing::warn!(%err, "unable to insert passkey");
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

    let passkeys = Credential::passkeys(&db).await.map_err(|err| {
        tracing::warn!(%err, "unable to select passkeys");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if passkeys.is_empty() {
        return Ok(Redirect::to("/register").into_response());
    }

    Ok(Page(LoginPage {}).into_response())
}

async fn login_start(
    db: Extension<SqlitePool>,
    webauthn: Extension<Arc<Webauthn>>,
    mut session: WritableSession,
) -> Result<Json<RequestChallengeResponse>, StatusCode> {
    let passkeys = Credential::passkeys(&db).await.map_err(|err| {
        tracing::warn!(%err, "unable to select passkeys");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Create a registration challenge.
    let (challenge, state) = webauthn.start_passkey_authentication(&passkeys).map_err(|err| {
        tracing::warn!(%err, "unable to start passkey authentication");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Store the authentication state in the session.
    session.remove("auth_state");
    session.insert("auth_state", &state).map_err(|err| {
        tracing::warn!(%err, "unable to store passkey authentication state in session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(challenge))
}

async fn login_finish(
    db: Extension<SqlitePool>,
    webauthn: Extension<Arc<Webauthn>>,
    mut session: WritableSession,
    Json(auth): Json<PublicKeyCredential>,
) -> Result<Response, StatusCode> {
    let state = session.get::<PasskeyAuthentication>("auth_state");
    session.remove("auth_state");

    let state = state.ok_or_else(|| {
        tracing::warn!("no stored authentication state");
        StatusCode::BAD_REQUEST
    })?;

    let res = webauthn.finish_passkey_authentication(&auth, &state).map_err(|err| {
        tracing::warn!(?err, "unable to finish passkey authentication");
        StatusCode::BAD_REQUEST
    })?;
    Credential::update(&db, &res).await.map_err(|err| {
        tracing::warn!(?err, "unable to update passkey");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    session.insert("authenticated", true).map_err(|err| {
        tracing::warn!(%err, "unable to store authentication state in session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::ACCEPTED.into_response())
}

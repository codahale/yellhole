use askama::Template;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_sessions::extractors::WritableSession;
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, PasskeyRegistration, RegisterPublicKeyCredential,
    RequestChallengeResponse,
};

use crate::models::Credential;

use super::{Context, Page};

pub fn router() -> Router {
    Router::new()
        .route("/register", get(register))
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        .route("/login", get(login))
        .route("/login/start", post(login_start))
        .route("/login/finish", post(login_finish))
}

#[derive(Debug, Template)]
#[template(path = "register.html")]
struct RegisterPage {}

async fn register() -> Result<Page<RegisterPage>, StatusCode> {
    Ok(Page(RegisterPage {}))
}

async fn register_start(
    ctx: Extension<Context>,
    mut session: WritableSession,
) -> Result<Json<CreationChallengeResponse>, StatusCode> {
    let existing_credentials = Credential::credential_ids(&ctx.db).await.map_err(|err| {
        tracing::warn!(%err, "unable to select credential IDs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Create a registration challenge.
    let (challenge, state) = ctx
        .webauthn
        .start_passkey_registration(
            Uuid::default(),
            &ctx.author,
            &ctx.author,
            Some(existing_credentials),
        )
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
    ctx: Extension<Context>,
    mut session: WritableSession,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Result<Response, StatusCode> {
    let state = session.get::<PasskeyRegistration>("reg_state");
    session.remove("reg_state");

    let state = state.ok_or_else(|| {
        tracing::warn!("no stored registration state");
        StatusCode::BAD_REQUEST
    })?;

    let passkey = ctx.webauthn.finish_passkey_registration(&reg, &state).map_err(|err| {
        tracing::warn!(?err, "unable to finish passkey registration");
        StatusCode::BAD_REQUEST
    })?;

    Credential::create(&ctx.db, passkey).await.map_err(|err| {
        tracing::warn!(%err, "unable to insert passkey");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::CREATED.into_response())
}

#[derive(Debug, Template)]
#[template(path = "login.html")]
struct LoginPage {}

async fn login() -> Result<Page<LoginPage>, StatusCode> {
    Ok(Page(LoginPage {}))
}

async fn login_start() -> Result<Json<RequestChallengeResponse>, StatusCode> {
    todo!()
}

async fn login_finish() -> Result<Response, StatusCode> {
    todo!()
}

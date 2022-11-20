use askama::Template;
use axum::extract::{FromRequest, RequestParts};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_sessions::extractors::{ReadableSession, WritableSession};
use uuid::Uuid;

use super::Page;
use crate::config::Config;
use crate::services::passkeys::{
    AuthenticationChallenge, AuthenticationResponse, PasskeyService, RegistrationChallenge,
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
    passkeys: Extension<PasskeyService>,
    session: ReadableSession,
) -> Result<Response, StatusCode> {
    let registered = passkeys.any_registered().await.map_err(|err| {
        tracing::warn!(%err, "unable to query DB");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if registered && !session.get::<bool>("authenticated").unwrap_or(false) {
        return Ok(Redirect::to("/login").into_response());
    }

    Ok(Page(RegisterPage {}).into_response())
}

async fn register_start(
    passkeys: Extension<PasskeyService>,
    config: Extension<Config>,
) -> Result<Json<RegistrationChallenge>, StatusCode> {
    passkeys
        .start_registration(&config.author, Uuid::default().as_hyphenated().to_string().as_bytes())
        .await
        .map(Json)
        .map_err(|err| {
            tracing::warn!(%err, "unable to start passkey registration");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn register_finish(
    passkeys: Extension<PasskeyService>,
    Json(resp): Json<RegistrationResponse>,
) -> Result<Response, StatusCode> {
    passkeys.finish_registration(resp).await.map_err(|err| {
        tracing::warn!(%err, "unable to finish passkey registration");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::CREATED.into_response())
}

#[derive(Debug, Template)]
#[template(path = "login.html")]
struct LoginPage {}

async fn login(
    passkeys: Extension<PasskeyService>,
    session: ReadableSession,
) -> Result<Response, StatusCode> {
    if session.get::<bool>("authenticated").unwrap_or(false) {
        return Ok(Redirect::to("/admin/new").into_response());
    }

    let registered = passkeys.any_registered().await.map_err(|err| {
        tracing::warn!(%err, "unable to query DB");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !registered {
        return Ok(Redirect::to("/register").into_response());
    }

    Ok(Page(LoginPage {}).into_response())
}

async fn login_start(
    passkeys: Extension<PasskeyService>,
    mut session: WritableSession,
) -> Result<Json<AuthenticationChallenge>, StatusCode> {
    let resp = passkeys.start_authentication().await.map_err(|err| {
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
    passkeys: Extension<PasskeyService>,
    mut session: WritableSession,
    Json(auth): Json<AuthenticationResponse>,
) -> Result<Response, StatusCode> {
    let challenge = session.get::<[u8; 32]>("challenge").ok_or_else(|| {
        tracing::warn!("no stored authentication state");
        StatusCode::BAD_REQUEST
    })?;
    session.remove("challenge");

    let authenticated = passkeys.finish_authentication(auth, challenge).await.map_err(|err| {
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

#[cfg(test)]
mod tests {
    use axum::{http, middleware};
    use axum_sessions::async_session::MemoryStore;
    use axum_sessions::SessionLayer;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use p256::PublicKey;
    use rand::thread_rng;
    use sha2::{Digest, Sha256};
    use spki::EncodePublicKey;
    use sqlx::SqlitePool;
    use url::Url;

    use crate::test_server::TestServer;

    use super::*;

    #[sqlx::test]
    async fn fresh_login_page(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(db))?;

        let resp = ts.get("/login").send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers().get(http::header::LOCATION),
            Some(&http::HeaderValue::from_static("/register"))
        );

        Ok(())
    }

    #[sqlx::test]
    async fn fresh_register_page(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(db))?;

        let resp = ts.get("/register").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }

    #[sqlx::test(fixtures("fake_passkey"))]
    async fn registered_register_page(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(db))?;

        let resp = ts.get("/register").send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers().get(http::header::LOCATION),
            Some(&http::HeaderValue::from_static("/login"))
        );

        Ok(())
    }

    #[sqlx::test(fixtures("fake_passkey"))]
    async fn registered_login_page(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(db))?;

        let resp = ts.get("/login").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }

    #[sqlx::test]
    async fn passkey_registration_and_login(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestServer::new(app(db))?;

        // Try a protected route. We should be blocked.
        let protected = ts.get("/protected").send().await?;
        assert_eq!(protected.status(), StatusCode::SEE_OTHER);

        // Generate a P-256 ECDSA key pair.
        let signing_key = SigningKey::random(&mut thread_rng());
        let public_key =
            PublicKey::from(signing_key.verifying_key()).to_public_key_der()?.into_vec();
        let key_id = Sha256::new().chain_update(&public_key).finalize().to_vec();

        // Start the registration process.
        let reg_start =
            ts.post("/register/start").send().await?.json::<RegistrationChallenge>().await?;

        // Generate the authenticator data.
        let mut authenticator_data = Vec::new();
        authenticator_data.extend(Sha256::new().chain_update(&reg_start.rp_id).finalize());
        authenticator_data.extend([1]); // flags
        authenticator_data.extend([0; 20]); // unused
        authenticator_data.extend(32u16.to_be_bytes());
        authenticator_data.extend(&key_id);

        // Register our public key.
        let reg_finish = ts
            .post("/register/finish")
            .json(&RegistrationResponse {
                authenticator_data,
                client_data_json: r#"{"type":"webauthn.create","origin":"http://example.com"}"#
                    .as_bytes()
                    .to_vec(),
                public_key,
            })
            .send()
            .await?;
        assert_eq!(reg_finish.status(), StatusCode::CREATED);

        // Start the login process.
        let login_start = ts.post("/login/start").send().await?;
        let login_start = login_start.json::<AuthenticationChallenge>().await?;
        assert!(login_start.passkey_ids.contains(&key_id));

        // Generate the collected client data and authenticator data.
        let cdj = format!(
            "{{\"type\":\"webauthn.get\",\"origin\":\"http://example.com\",\"challenge\": {:?}}}",
            base64::encode(login_start.challenge)
        );

        let mut authenticator_data = Vec::new();
        authenticator_data.extend(Sha256::new().chain_update(&login_start.rp_id).finalize());
        authenticator_data.extend([1]); // flags
        authenticator_data.extend([0; 20]); // unused
        authenticator_data.extend(32u16.to_be_bytes());
        authenticator_data.extend(&key_id);

        // Sign authenticator data and a hash of the collected client data.
        let mut signed = authenticator_data.clone();
        signed.extend(Sha256::new().chain_update(&cdj).finalize());
        let signature = signing_key.sign(&signed).to_der();

        // Send our signature to authenticate.
        let login_finish = ts
            .post("/login/finish")
            .json(&AuthenticationResponse {
                raw_id: key_id.clone(),
                authenticator_data,
                client_data_json: cdj.as_bytes().to_vec(),
                signature: signature.as_bytes().to_vec(),
            })
            .send()
            .await?;
        assert_eq!(login_finish.status(), StatusCode::ACCEPTED);

        // Try the protected resource again.
        let protected = ts.get("/protected").send().await?;
        assert_eq!(protected.status(), StatusCode::OK);

        Ok(())
    }

    fn app(db: SqlitePool) -> Router {
        let store = MemoryStore::new();
        let session_layer = SessionLayer::new(store, &[69; 64])
            .with_secure(false)
            .with_same_site_policy(axum_sessions::SameSite::None);
        let base_url = "http://example.com".parse::<Url>().unwrap();
        Router::new()
            .route("/protected", get(protected))
            .route_layer(middleware::from_extractor::<RequireAuth>())
            .merge(router())
            .layer(Extension(PasskeyService::new(db, &base_url)))
            .layer(Extension(Config {
                port: 8080,
                base_url,
                data_dir: ".".into(),
                title: "Yellhole".into(),
                author: "Luther Blissett".into(),
            }))
            .layer(session_layer)
    }

    async fn protected() -> &'static str {
        "secure"
    }
}

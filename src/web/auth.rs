use std::time::Duration;

use askama::Template;
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};

use crate::{
    id::PublicId,
    services::{
        passkeys::{
            AuthenticationChallenge, AuthenticationResponse, PasskeyError, PasskeyService,
            RegistrationChallenge, RegistrationResponse,
        },
        sessions::SessionService,
    },
    web::app::{AppError, AppState, Page},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", get(register))
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        .route("/login", get(login))
        .route("/login/start", post(login_start))
        .route("/login/finish", post(login_finish))
}

pub async fn require_auth(
    state: State<AppState>,
    cookies: CookieJar,
    req: Request<Body>,
    next: Next,
) -> Response {
    match is_authenticated(&state, &cookies).await {
        Ok(true) => next.run(req).await,
        _ => {
            tracing::warn!("unauthenticated request");
            Redirect::to("/login").into_response()
        }
    }
}

#[derive(Debug, Template)]
#[template(path = "register.html")]
struct RegisterPage {}

async fn register(state: State<AppState>, cookies: CookieJar) -> Result<Response, AppError> {
    if state.passkeys.any_registered().await? && !is_authenticated(&state, &cookies).await? {
        return Ok(Redirect::to("/login").into_response());
    }

    Ok(Page(RegisterPage {}).into_response())
}

async fn register_start(state: State<AppState>) -> Result<Json<RegistrationChallenge>, AppError> {
    Ok(state
        .passkeys
        .start_registration(&state.config.author, PublicId::random().to_string().as_bytes())
        .await
        .map(Json)?)
}

async fn register_finish(
    state: State<AppState>,
    Json(resp): Json<RegistrationResponse>,
) -> Result<Response, AppError> {
    match state.passkeys.finish_registration(resp).await {
        Ok(()) => Ok(StatusCode::CREATED.into_response()),
        Err(PasskeyError::DatabaseError(err)) => Err(AppError::QueryFailure(err)),
        Err(_) => Ok(StatusCode::BAD_REQUEST.into_response()),
    }
}

#[derive(Debug, Template)]
#[template(path = "login.html")]
struct LoginPage {}

async fn login(state: State<AppState>, cookies: CookieJar) -> Result<Response, AppError> {
    if is_authenticated(&state, &cookies).await? {
        return Ok(Redirect::to("/admin/new").into_response());
    }

    if !state.passkeys.any_registered().await? {
        return Ok(Redirect::to("/register").into_response());
    }

    Ok(Page(LoginPage {}).into_response())
}

async fn login_start(
    state: State<AppState>,
    cookies: CookieJar,
) -> Result<(CookieJar, Json<AuthenticationChallenge>), AppError> {
    let (challenge_id, resp) = state.passkeys.start_authentication().await?;
    let cookies = cookies.add(cookie(&state, "challenge", challenge_id, PasskeyService::TTL));
    Ok((cookies, Json(resp)))
}

async fn login_finish(
    state: State<AppState>,
    cookies: CookieJar,
    Json(auth): Json<AuthenticationResponse>,
) -> Result<(CookieJar, StatusCode), AppError> {
    let Some(challenge_id) = cookies.get("challenge").and_then(|c| c.value().parse().ok()) else {
        return Ok((cookies, StatusCode::BAD_REQUEST));
    };

    let cookies = cookies.remove(Cookie::build(("challenge", "")).path("/"));
    match state.passkeys.finish_authentication(auth, challenge_id).await {
        Ok(()) => {
            let session_id = state.sessions.create().await?;
            let cookies = cookies.add(cookie(&state, "session", session_id, SessionService::TTL));
            Ok((cookies, StatusCode::ACCEPTED))
        }
        Err(PasskeyError::DatabaseError(err)) => Err(AppError::QueryFailure(err)),
        Err(_) => Ok((cookies, StatusCode::BAD_REQUEST)),
    }
}

fn cookie<'c>(state: &AppState, name: &'c str, value: PublicId, max_age: Duration) -> Cookie<'c> {
    Cookie::build((name, value.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(max_age.try_into().expect("invalid duration"))
        .secure(state.config.base_url.scheme() == "https")
        .path("/")
        .into()
}

async fn is_authenticated(state: &AppState, cookies: &CookieJar) -> Result<bool, anyhow::Error> {
    match cookies.get("session") {
        Some(cookie) => Ok(state.sessions.exists(cookie.value().parse()?).await?),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use axum::middleware;
    use p256::{
        PublicKey,
        ecdsa::{Signature, SigningKey, signature::Signer},
        pkcs8::EncodePublicKey,
    };
    use rand::thread_rng;
    use reqwest::{StatusCode, header};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{services::passkeys::CollectedClientData, test::TestEnv};

    #[tokio::test]
    async fn fresh_pages() -> Result<(), anyhow::Error> {
        let env = TestEnv::new().await?;
        let app = app(&env.state);
        let ts = env.into_server(app).await?;

        let resp = ts.get("/register").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = ts.get("/login").send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers().get(header::LOCATION).map(|h| h.as_bytes()),
            Some("/register".as_bytes())
        );

        Ok(())
    }

    #[tokio::test]
    async fn registered_pages() -> Result<(), anyhow::Error> {
        let env = TestEnv::new().await?;
        let app = app(&env.state);
        let ts = env.into_server(app).await?;
        ts.db
            .call_unwrap(|conn| {
                conn.execute_batch(
                    r#"
insert into passkey (passkey_id, public_key_spki) values (randomblob(16), randomblob(33));
        "#,
                )
            })
            .await?;

        let resp = ts.get("/register").send().await?;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers().get(header::LOCATION).map(|h| h.as_bytes()),
            Some("/login".as_bytes())
        );

        let resp = ts.get("/login").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn passkey_registration_and_login() -> Result<(), anyhow::Error> {
        let env = TestEnv::new().await?;
        let app = app(&env.state);
        let ts = env.into_server(app).await?;

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
        let client_data_json = serde_json::to_vec(&CollectedClientData {
            challenge: None,
            origin: "http://example.com".parse()?,
            type_: "webauthn.create".into(),
            cross_origin: Some(false),
        })?;
        let reg_finish = ts
            .post("/register/finish")
            .json(&RegistrationResponse { authenticator_data, client_data_json, public_key })
            .send()
            .await?;
        assert_eq!(reg_finish.status(), StatusCode::CREATED);

        // Start the login process.
        let login_start = ts.post("/login/start").send().await?;
        let login_start = login_start.json::<AuthenticationChallenge>().await?;
        assert!(login_start.passkey_ids.contains(&key_id));

        // Generate the collected client data and authenticator data.
        let client_data_json = serde_json::to_vec(&CollectedClientData {
            challenge: Some(login_start.challenge.to_vec()),
            origin: "http://example.com".parse()?,
            type_: "webauthn.get".into(),
            cross_origin: Some(false),
        })?;

        let mut authenticator_data = Vec::new();
        authenticator_data.extend(Sha256::new().chain_update(&login_start.rp_id).finalize());
        authenticator_data.extend([1]); // flags
        authenticator_data.extend([0; 20]); // unused
        authenticator_data.extend(32u16.to_be_bytes());
        authenticator_data.extend(&key_id);

        // Sign authenticator data and a hash of the collected client data.
        let mut signed = authenticator_data.to_vec();
        signed.extend(Sha256::new().chain_update(&client_data_json).finalize());
        let signature: Signature = signing_key.sign(&signed);

        // Send our signature to authenticate.
        let login_finish = ts
            .post("/login/finish")
            .json(&AuthenticationResponse {
                raw_id: key_id,
                client_data_json,
                authenticator_data,
                signature: signature.to_der().as_bytes().to_vec(),
            })
            .send()
            .await?;
        assert_eq!(login_finish.status(), StatusCode::ACCEPTED);

        // Try the protected resource again.
        let protected = ts.get("/protected").send().await?;
        assert_eq!(protected.status(), StatusCode::OK);

        Ok(())
    }

    fn app(state: &AppState) -> Router<AppState> {
        Router::<AppState>::new()
            .route("/protected", get(protected))
            .route_layer(middleware::from_fn_with_state(state.clone(), super::require_auth))
            .merge(router())
    }

    async fn protected() -> &'static str {
        "secure"
    }
}

use constant_time_eq::constant_time_eq;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_with::base64::{Base64, UrlSafe};
use serde_with::formats::Unpadded;
use serde_with::{serde_as, PickFirst};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use url::Url;
use uuid::Uuid;

pub async fn any_registered(db: &SqlitePool) -> Result<bool, sqlx::Error> {
    sqlx::query!(r"select count(passkey_id) as n from passkey").fetch_one(db).await.map(|r| r.n > 0)
}

#[serde_as]
#[derive(Debug, Serialize)]
pub struct RegistrationChallenge {
    #[serde(rename = "rpId")]
    rp_id: String,

    #[serde(rename = "userIdBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    user_id: Vec<u8>,

    username: String,

    #[serde(rename = "passkeyIdsBase64")]
    #[serde_as(as = "Vec<PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>>")]
    passkey_ids: Vec<Vec<u8>>,
}

pub async fn start_registration(
    db: &SqlitePool,
    base_url: &Url,
    username: &str,
) -> Result<RegistrationChallenge, sqlx::Error> {
    // Return a registration challenge with an all-zero UUID as the user ID.
    Ok(RegistrationChallenge {
        rp_id: base_url.host_str().unwrap().into(),
        username: username.into(),
        user_id: Uuid::default().as_hyphenated().to_string().into_bytes(),
        passkey_ids: passkey_ids(db).await?,
    })
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct RegistrationResponse {
    #[serde(rename = "clientDataJSONBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    client_data_json: Vec<u8>,

    #[serde(rename = "authenticatorDataBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    authenticator_data: Vec<u8>,

    #[serde(rename = "publicKeyBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    public_key: Vec<u8>,
}

pub async fn finish_registration(
    db: &SqlitePool,
    base_url: &Url,
    resp: RegistrationResponse,
) -> Result<(), anyhow::Error> {
    // Try decoding the P-256 public key from its DER encoding.
    VerifyingKey::from_public_key_der(&resp.public_key)?;

    // Decode and validate the client data.
    if !serde_json::from_slice::<CollectedClientData>(&resp.client_data_json)
        .map(|cdj| {
            cdj.type_ == "webauthn.create"
                && !cdj.cross_origin.unwrap_or(false)
                && &cdj.origin == base_url
        })
        .unwrap_or(false)
    {
        anyhow::bail!("invalid client data");
    }

    // Decode and validate the authenticator data.
    let flags = resp.authenticator_data[32];
    anyhow::ensure!(flags & 1 == 1, "user must be present for passkey registration");
    anyhow::ensure!(resp.authenticator_data.len() > 55, "credential ID must be included");
    let cred_id_len =
        u16::from_be_bytes(resp.authenticator_data[53..55].try_into().unwrap()) as usize;
    anyhow::ensure!(resp.authenticator_data.len() > 55 + cred_id_len, "bad credential ID size");
    let passkey_id = resp.authenticator_data[55..55 + cred_id_len].to_vec();

    // Insert the passkey ID and DER-encoded public key into the database.
    sqlx::query!(
        r"insert into passkey (passkey_id, public_key_spki) values (?, ?)",
        passkey_id,
        resp.public_key,
    )
    .execute(db)
    .await?;

    Ok(())
}

#[serde_as]
#[derive(Debug, Deserialize)]
struct CollectedClientData {
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    challenge: Option<Vec<u8>>,
    origin: Url,

    #[serde(rename = "type")]
    type_: String,

    #[serde(rename = "crossOrigin")]
    cross_origin: Option<bool>,
}

#[serde_as]
#[derive(Debug, Serialize)]
pub struct AuthenticationChallenge {
    #[serde(rename = "rpId")]
    rp_id: String,

    #[serde(rename = "challengeBase64")]
    #[serde_as(as = "Base64")]
    challenge: [u8; 32],

    #[serde(rename = "passkeyIdsBase64")]
    #[serde_as(as = "Vec<PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>>")]
    passkey_ids: Vec<Vec<u8>>,
}

pub async fn start_authentication(
    db: &SqlitePool,
    base_url: &Url,
) -> Result<(AuthenticationChallenge, [u8; 32]), sqlx::Error> {
    // Find all passkey IDs.
    let passkey_ids = passkey_ids(db).await?;

    // Generate a random challenge.
    let challenge = thread_rng().gen::<[u8; 32]>();

    Ok((
        AuthenticationChallenge {
            rp_id: base_url.host_str().unwrap().into(),
            challenge,
            passkey_ids,
        },
        challenge,
    ))
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct AuthenticationResponse {
    #[serde(rename = "rawIdBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    raw_id: Vec<u8>,

    #[serde(rename = "clientDataJSONBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    client_data_json: Vec<u8>,

    #[serde(rename = "authenticatorDataBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    authenticator_data: Vec<u8>,

    #[serde(rename = "signatureBase64")]
    #[serde_as(as = "PickFirst<(Base64, Base64<UrlSafe, Unpadded>)>")]
    signature: Vec<u8>,
}

pub async fn finish_authentication(
    db: &SqlitePool,
    base_url: &Url,
    resp: AuthenticationResponse,
    challenge: [u8; 32],
) -> Result<bool, sqlx::Error> {
    // Validate the collected client data.
    if !serde_json::from_slice::<CollectedClientData>(&resp.client_data_json)
        .map(|cdj| {
            cdj.type_ == "webauthn.get"
                && !cdj.cross_origin.unwrap_or(false)
                && &cdj.origin == base_url
                && constant_time_eq(&cdj.challenge.unwrap_or_default(), &challenge)
        })
        .unwrap_or(false)
    {
        tracing::warn!(cdj=?resp.client_data_json, "invalid collected client data");
        return Ok(false);
    }

    // Decode and validate the authenticator data.
    let flags = resp.authenticator_data[32];
    if flags & 1 == 0 {
        tracing::warn!(?flags, "user not present for passkey auth");
        return Ok(false);
    }
    let rp_hash = Sha256::new().chain_update(base_url.host_str().unwrap().as_bytes()).finalize();
    if !constant_time_eq(&rp_hash, &resp.authenticator_data[..32]) {
        tracing::warn!(?resp.authenticator_data, "invalid authenticator data");
        return Ok(false);
    }

    // Find the passkey by ID.
    let Some(public_key_spki) = sqlx::query!(
        r"select public_key_spki from passkey where passkey_id = ?",
        resp.raw_id,
    )
    .fetch_optional(db)
    .await?
    .map(|r| r.public_key_spki) else {
        tracing::warn!(passkey_id=?resp.raw_id, "unable to find passkey");
        return Ok(false);
    };

    // Decode the public key.
    let Ok(public_key) = VerifyingKey::from_public_key_der(&public_key_spki) else {
        tracing::warn!(passkey_id=?resp.raw_id, "unable to decode public key");
        return Ok(false);
    };

    // Re-calculate the signed material.
    let mut signed = resp.authenticator_data.clone();
    let cdj_hash = Sha256::new().chain_update(&resp.client_data_json).finalize();
    signed.extend(cdj_hash);

    // Decode the signature.
    let Ok(signature) = Signature::from_der(resp.signature.as_slice()) else {
        tracing::warn!(passkey_id=?resp.raw_id, "unable to decode signature");
        return Ok(false);
    };

    // Verify the signature.
    Ok(public_key.verify(&signed, &signature).is_ok())
}

async fn passkey_ids(db: &SqlitePool) -> Result<Vec<Vec<u8>>, sqlx::Error> {
    Ok(sqlx::query!(r"select passkey_id from passkey")
        .fetch_all(db)
        .await?
        .into_iter()
        .map(|r| r.passkey_id)
        .collect::<Vec<Vec<u8>>>())
}

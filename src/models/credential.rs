#![allow(unused)] // TODO remove this

use chrono::NaiveDateTime;
use sqlx::SqlitePool;
use webauthn_rs::prelude::{AuthenticationResult, CredentialID, Passkey};

#[derive(Debug)]
pub struct Credential {
    pub credential_id: String,
    as_json: String,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
}

impl Credential {
    pub async fn by_id(
        db: &SqlitePool,
        id: &CredentialID,
    ) -> Result<Option<Credential>, sqlx::Error> {
        let credential_id = serde_json::to_string(id).expect("invalid credential ID");
        sqlx::query_as!(
            Credential,
            r"
            select credential_id, as_json, created_at, updated_at
            from credential
            where credential_id = ?
            ",
            credential_id,
        )
        .fetch_optional(db)
        .await
    }

    pub async fn credential_ids(db: &SqlitePool) -> Result<Vec<CredentialID>, sqlx::Error> {
        Ok(sqlx::query!(
            r"
            select credential_id
            from credential
            "
        )
        .fetch_all(db)
        .await?
        .into_iter()
        .flat_map(|r| serde_json::from_str::<CredentialID>(&r.credential_id).ok())
        .collect::<Vec<CredentialID>>())
    }

    pub async fn create(db: &SqlitePool, passkey: Passkey) -> Result<(), sqlx::Error> {
        let credential_id =
            serde_json::to_string(&passkey.cred_id()).expect("invalid credential ID");
        let as_json = serde_json::to_string(&passkey).expect("invalid passkey");
        sqlx::query!(
            r"insert into credential (credential_id, as_json) values (?, ?)",
            credential_id,
            as_json
        )
        .execute(db)
        .await?;
        Ok(())
    }

    pub async fn update(db: &SqlitePool, auth: &AuthenticationResult) -> Result<(), sqlx::Error> {
        if !auth.needs_update() {
            return Ok(());
        }

        if let Some(credential) = Self::by_id(db, auth.cred_id()).await? {
            let mut passkey = credential.to_passkey();
            if passkey.update_credential(auth).unwrap_or(false) {
                let credential_id =
                    serde_json::to_string(auth.cred_id()).expect("invalid credential ID");
                let as_json = serde_json::to_string(&passkey).expect("invalid passkey");
                sqlx::query!(
                    r"
                    update credential
                    set as_json = ?, updated_at = current_timestamp
                    where credential_id = ?
                    ",
                    as_json,
                    credential_id,
                )
                .execute(db)
                .await?;
            }
        }

        Ok(())
    }

    pub fn to_passkey(&self) -> Passkey {
        serde_json::from_str(&self.as_json).expect("invalid passkey")
    }
}

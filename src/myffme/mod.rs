pub mod address;
mod document;
pub mod email;
mod health_questionnaire;
pub mod license;
mod medical_certificate;
pub mod member;
mod options;
pub mod price;
mod product;
mod structure;

use crate::http_client::json_client;
use pinboard::Pinboard;
use reqwest::header::{HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tracing::warn;

pub(crate) struct Authorization {
    pub(crate) bearer_token: HeaderValue,
    pub(crate) timestamp: u32,
}

#[derive(Deserialize)]
struct Token {
    token: String,
}

pub(crate) const MYFFME_AUTHORIZATION_VALIDITY_SECONDS: u32 = 36_000; // 10h

const USERNAME_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "MYFFME_USERNAME",
};
const PASSWORD_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "MYFFME_PASSWORD",
};

const STRUCTURE_ID_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "MYFFME_STRUCTURE_ID",
};

const X_HASURA_ROLE: HeaderName = HeaderName::from_static("x-hasura-role");
const ADMIN: HeaderValue = HeaderValue::from_static("admin");

static USERNAME: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(USERNAME_KEY).expect("myffme username not set"));
//noinspection SpellCheckingInspection
static PASSWORD: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(PASSWORD_KEY).expect("myffme password not set"));

pub static STRUCTURE_ID: LazyLock<u32> = LazyLock::new(|| {
    secret_value(STRUCTURE_ID_KEY)
        .expect("myffme structure id not set")
        .parse()
        .expect("invalid myffme structure id")
});

pub(crate) static MYFFME_AUTHORIZATION: LazyLock<Pinboard<Authorization>> =
    LazyLock::new(Pinboard::new_empty);

pub async fn update_myffme_bearer_token(timestamp: u32) -> Option<String> {
    match json_client()
        .post("https://app.myffme.fr/api/auth/login")
        .json(&json!({
            "username": *USERNAME,
            "password": *PASSWORD,
        }))
        .send()
        .await
    {
        Ok(response) => match response.json::<Token>().await {
            Ok(token) => {
                let bearer_token =
                    HeaderValue::try_from(format!("Bearer {}", token.token)).unwrap();
                MYFFME_AUTHORIZATION.set(Authorization {
                    bearer_token,
                    timestamp,
                });
                Some(token.token)
            }
            Err(err) => {
                warn!("failed to parse login response:\n{err:?}");
                None
            }
        },
        Err(err) => {
            warn!("failed to get login response:\n{err:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::member::{licensees, members_by_structure};
    use crate::season::current_season;
    use std::time::SystemTime;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    #[ignore]
    async fn test_bearer_token() {
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
    }

    #[tokio::test]
    async fn test_list() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let all_members = members_by_structure(*STRUCTURE_ID, None).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!all_members.is_empty());
        // println!("{}", all_members.len());
        // println!("{}", serde_json::to_string(&all_members).unwrap());
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(".members.json")
            .await
            .ok()
            .unwrap()
            .write_all(serde_json::to_string(&all_members).unwrap().as_bytes())
            .await
            .unwrap();
        let season = current_season(None);
        let t0 = SystemTime::now();
        let licensees = licensees(*STRUCTURE_ID, season).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(format!(".licensees_{season}.json"))
            .await
            .ok()
            .unwrap()
            .write_all(serde_json::to_string(&all_members).unwrap().as_bytes())
            .await
            .unwrap();
        for licensee in licensees {
            assert!(
                all_members
                    .iter()
                    .find(|it| it.metadata.myffme_user_id.as_ref().unwrap()
                        == licensee.metadata.myffme_user_id.as_ref().unwrap())
                    .is_some()
            )
        }
    }
}

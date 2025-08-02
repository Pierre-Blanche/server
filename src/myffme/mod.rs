pub mod address;
pub mod email;
mod graphql;
pub mod license;
mod licensee;
mod me;
pub mod price;
mod product;
mod structure;

use crate::http_client::json_client;
use crate::myffme::licensee::{licensees, Licensee};
use crate::order::{InsuranceLevel, InsuranceOption};
use crate::user::Metadata;
use license::{
    deserialize_insurance_level, deserialize_insurance_option, deserialize_license_type,
};
use pinboard::Pinboard;
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::ops::Deref;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::norm::{normalize_first_name, normalize_last_name};
use tiered_server::store::Snapshot;
use tiered_server::user::{Email, IdentificationMethod, User};
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize)]
pub struct Member {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub dob: u32,
    pub metadata: Metadata,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Female,
    Male,
    Unspecified,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    Adult,
    Child,
    Family,
    NonMemberAdult,
    NonMemberChild,
    NonPracticing,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MedicalCertificateStatus {
    Recreational,
    Competition,
    HealthQuestionnaire,
    WaitingForDocument,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Structure {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub department: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Competition {
    pub season: u16,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompetitionResult {
    pub rank: u16,
    pub category_name: String,
    pub competition: Competition,
}

#[derive(Deserialize)]
pub(crate) struct License {
    pub user_id: Option<String>,
    pub season: u16,
    pub license_number: u32,
    pub structure_id: u32,
    #[serde(rename = "product_id", deserialize_with = "deserialize_license_type")]
    pub license_type: LicenseType,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct LicenseFees {
    pub federal_fee_in_cents: u16,
    pub regional_fee_in_cents: u16,
    pub department_fee_in_cents: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InsuranceLevelOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_level")]
    pub level: InsuranceLevel,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InsuranceOptionOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_option")]
    pub option: InsuranceOption,
}

pub(crate) struct Authorization {
    pub(crate) bearer_token: HeaderValue,
    pub(crate) timestamp: u32,
}

#[derive(Deserialize)]
pub struct Token {
    pub token: String,
    #[serde(alias = "refreshToken")]
    pub(crate) refresh_token: String,
}

impl Deref for Token {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.token.as_str()
    }
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

pub(crate) static USERNAME: LazyLock<&'static str> =
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

pub async fn update_myffme_bearer_token(
    timestamp: u32,
    refresh_token: Option<String>,
) -> Option<Token> {
    let client = json_client();
    if let Some(refresh_token) = refresh_token {
        match client
            .post("https://api.core.myffme.fr/auth/refresh")
            .json(&json!({
                "refreshToken": refresh_token,
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
                    return Some(token);
                }
                Err(err) => {
                    warn!("failed to parse token refresh response:\n{err:?}");
                }
            },
            Err(err) => {
                warn!("failed to get token refresh response:\n{err:?}");
            }
        }
    }
    match client
        .post("https://api.core.myffme.fr/auth/login")
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
                Some(token)
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

pub(crate) async fn add_missing_users(
    snapshot: &Snapshot,
    log: bool,
) -> Result<Option<String>, String> {
    let mut output = if log { Some(String::new()) } else { None };
    let existing_users = snapshot
        .list::<User>("acc/")
        .map(|(_, it)| it)
        .collect::<Vec<_>>();
    info!("existing users: {}", existing_users.len());
    if let Some(output) = output.as_mut() {
        let _ = writeln!(output, "existing users: {}", existing_users.len());
    }
    let existing_users_metadata = existing_users
        .iter()
        .flat_map(|it| {
            it.metadata
                .as_ref()
                .and_then(|value| Metadata::deserialize(value).ok())
        })
        .collect::<Vec<_>>();
    let lookup = existing_users_metadata
        .iter()
        .filter_map(|it| it.myffme_user_id.as_ref().map(|id| (id, it)))
        .collect::<BTreeMap<_, _>>();
    let licensees = licensees().await.ok_or("failed to get licensees")?;
    info!("licensees: {}", licensees.len());
    for licensee in licensees {
        let Licensee {
            myffme_user_id,
            license_number,
            first_name,
            last_name,
            email,
            dob,
            ..
        } = licensee;
        if lookup.contains_key(&myffme_user_id) {
            continue;
        }
        let metadata = Metadata {
            myffme_user_id: Some(myffme_user_id),
            license_number: Some(license_number),
            ..Default::default()
        };
        let normalized_first_name = normalize_first_name(first_name.as_str());
        let normalized_last_name = normalize_last_name(last_name.as_str());
        let last = existing_users
            .iter()
            .filter(|&it| {
                it.date_of_birth == dob
                    && it.normalized_first_name == normalized_first_name
                    && (it.normalized_last_name == normalized_last_name
                        || it.email().unwrap_or_default() == email)
            })
            .enumerate()
            .last();
        if let Some((i, it)) = last {
            if i == 0 {
                // only one match, set myffme_user_id and license_number
                let mut user = it.clone();
                user.metadata = Some(
                    serde_json::to_value(metadata)
                        .map_err(|_| "failed to serialize metadata".to_string())?,
                );
                match Snapshot::set_and_return_before_update(&format!("acc/{}", user.id), &user)
                    .await
                {
                    Some(_) => continue,
                    None => return Err(format!("failed to assign licensee to user {}", user.id)),
                }
            } else {
                // multiple matches, abort
                return Err(format!("multiple users found for {first_name} {last_name}"));
            }
        }
        // no match, create user
        let id = User::new_id(0);
        let key = format!("acc/{id}");
        info!("adding {first_name} {last_name}");
        if let Some(output) = output.as_mut() {
            let _ = writeln!(output, "adding {first_name} {last_name}");
        }
        let identification = IdentificationMethod::Email(Email::from(email));
        let user = User {
            id,
            identification: vec![identification],
            last_name,
            normalized_last_name,
            first_name,
            normalized_first_name,
            date_of_birth: dob,
            admin: false,
            metadata: Some(
                serde_json::to_value(metadata)
                    .map_err(|_| "failed to serialize metadata".to_string())?,
            ),
        };
        Snapshot::set_and_return_before_update(key.as_str(), &user)
            .await
            .ok_or("failed to add user".to_string())?;
    }
    Ok(output)
}

pub(crate) async fn update_users_metadata(
    _snapshot: &Snapshot,
    _log: bool,
) -> Result<Option<String>, String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_bearer_token() {
        let token = update_myffme_bearer_token(0, None)
            .await
            .expect("failed to get bearer token");
        let refreshed = update_myffme_bearer_token(0, Some(token.refresh_token.clone()))
            .await
            .expect("failed to refresh bearer token");
        assert_ne!(token.token, refreshed.token);
        assert_ne!(token.refresh_token, refreshed.refresh_token);
        println!("token:{}", token.deref());
    }
}

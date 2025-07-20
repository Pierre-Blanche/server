pub mod address;
pub mod email;
mod graphql;
pub mod license;
pub mod prices;

use crate::http_client::json_client;
use crate::order::{InsuranceLevel, InsuranceOption};
use license::{
    deserialize_insurance_level, deserialize_insurance_option, deserialize_license_type,
};
use pinboard::Pinboard;
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::store::Snapshot;
use tracing::warn;

#[derive(Deserialize, Serialize)]
pub struct Member {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub dob: u32,
    pub metadata: Metadata,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Metadata {
    pub myffme_user_id: Option<String>,
    pub license_number: Option<u32>,
    pub gender: Option<Gender>,
    pub insee: Option<String>,
    pub city: Option<String>,
    pub zip_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_type: Option<LicenseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medical_certificate_status: Option<MedicalCertificateStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_license_season: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_structure: Option<Structure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competition_results: Option<Vec<CompetitionResult>>,
}

#[derive(Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Female,
    Male,
    Unspecified,
}

#[derive(Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    Adult,
    Child,
    Family,
    NonMemberAdult,
    NonMemberChild,
    NonPracticing,
}

#[derive(Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MedicalCertificateStatus {
    Recreational,
    Competition,
    HealthQuestionnaire,
    WaitingForDocument,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Structure {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub department: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Competition {
    pub season: u16,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct CompetitionResult {
    pub rank: u16,
    pub category_name: String,
    pub competition: Competition,
}

#[derive(Deserialize)]
pub(crate) struct License {
    pub user_id: Option<String>,
    pub season: u16,
    // #[serde(deserialize_with = "deserialize_license_number")]
    // pub license_number: u32,
    pub structure_id: u32,
    #[serde(rename = "product_id", deserialize_with = "deserialize_license_type")]
    pub license_type: Option<LicenseType>,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct LicenseFees {
    pub federal_fee_in_cents: u16,
    pub regional_fee_in_cents: u16,
    pub department_fee_in_cents: u16,
}

#[derive(Deserialize)]
pub(crate) struct InsuranceLevelOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_level")]
    pub level: Option<InsuranceLevel>,
}

#[derive(Deserialize)]
pub(crate) struct InsuranceOptionOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_option")]
    pub option: Option<InsuranceOption>,
}

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
        .post("https://app.myffme.fr/authentification/connexion")
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

pub(crate) async fn add_missing_users(
    _snapshot: &Snapshot,
    _season: Option<u16>,
    _log: bool,
) -> Result<Option<String>, String> {
    todo!()
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
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
    }
}

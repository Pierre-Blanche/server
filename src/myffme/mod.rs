pub mod address;
pub mod email;
// mod graphql;
pub mod license;
mod licensee;
mod me;
pub mod price;
mod product;
mod structure;

use crate::emergency_contact::EmergencyContact;
use crate::http_client::json_client;
use crate::mycompet::results::competition_results;
use crate::myffme::licensee::{
    address, emergency_contact, license, licensees, user_data, Licensee,
};
use crate::myffme::structure::structure_hierarchy_by_id;
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
use tiered_server::norm::{normalize_first_name, normalize_last_name, normalize_phone_number};
use tiered_server::store::Snapshot;
use tiered_server::user::{Email, IdentificationMethod, Sms, User};
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

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Structure {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub department: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Competition {
    pub season: u16,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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
                info!("assigning license to {first_name} {last_name}");
                if let Some(output) = output.as_mut() {
                    let _ = writeln!(output, "assigning license to {first_name} {last_name}");
                }
                let mut user = it.clone();
                user.metadata = Some(
                    serde_json::to_value(metadata)
                        .map_err(|_| "failed to serialize metadata".to_string())?,
                );
                match Snapshot::set_and_return_before_update(&format!("acc/{}", user.id), &user)
                    .await
                {
                    Some(_) => continue,
                    None => return Err(format!("failed to assign license to user {}", user.id)),
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
    snapshot: &Snapshot,
    log: bool,
) -> Result<Option<String>, String> {
    let mut output = if log { Some(String::new()) } else { None };
    let entries = snapshot
        .list::<User>("acc/")
        .map(|(k, v)| (k.to_string(), v))
        .collect::<Vec<_>>();
    info!("existing users: {}", entries.len());
    if let Some(output) = output.as_mut() {
        let _ = writeln!(output, "existing users: {}", entries.len());
    }
    let this_structure: Structure = structure_hierarchy_by_id(*STRUCTURE_ID)
        .await
        .ok_or("failed to get structure".to_string())?
        .into();
    for (key, mut user) in entries {
        let first_name = user.first_name.as_str();
        let last_name = user.last_name.as_str();
        if let Some(metadata) = user
            .metadata
            .take()
            .and_then(|it| serde_json::from_value::<Metadata>(it).ok())
        {
            if let Some(myffme_user_id) = metadata.myffme_user_id.as_ref() {
                let mut modified = false;
                let user_data = user_data(myffme_user_id).await.ok_or(format!(
                    "failed to get data for user {first_name} {last_name}"
                ))?;
                let latest_license = if let Some(paths) = user_data.license_paths.as_ref() {
                    if let Some(license_path) = paths.last() {
                        Some(license(license_path).await.ok_or(format!(
                            "failed to get license for user {first_name} {last_name}"
                        ))?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let latest_structure = if let Some(structure_id) =
                    latest_license.as_ref().map(|it| it.structure.structure)
                {
                    if structure_id == this_structure.id {
                        Some(this_structure.clone())
                    } else {
                        if let Some(it) = structure_hierarchy_by_id(structure_id).await {
                            Some(it.into())
                        } else {
                            warn!(
                                "failed to get structure {}",
                                latest_license.as_ref().unwrap().structure.name
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                let address = if let Some(paths) = user_data.address_paths.as_ref() {
                    if let Some(address_path) = paths.last() {
                        Some(address(address_path).await.ok_or(format!(
                            "failed to get address for user {first_name} {last_name}"
                        ))?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let emergency_contacts = if let Some(paths) =
                    user_data.emergency_contact_paths.as_ref()
                {
                    if paths.is_empty() {
                        None
                    } else {
                        let mut vec = Vec::with_capacity(paths.len());
                        for path in paths {
                            let it = emergency_contact(path).await.ok_or(format!(
                                "failed to get emergency contact for user {first_name} {last_name}"
                            ))?;
                            let mut identification_methods = Vec::with_capacity(2);
                            if let Some(email) = it.email {
                                identification_methods
                                    .push(IdentificationMethod::Email(Email::from(email)))
                            }
                            if let Some(number) = it.phone_number {
                                let normalized_number = normalize_phone_number(&number, 33);
                                if normalized_number.starts_with("+336")
                                    || normalized_number.starts_with("+337")
                                {
                                    identification_methods.push(IdentificationMethod::Sms(Sms {
                                        number,
                                        normalized_number,
                                    }))
                                }
                            }
                            vec.push(EmergencyContact {
                                normalized_first_name: normalize_first_name(&it.first_name),
                                first_name: it.first_name,
                                normalized_last_name: normalize_first_name(&it.last_name),
                                last_name: it.last_name,
                                relationship: it.relationship.unwrap_or_default(),
                                identification: identification_methods,
                            });
                        }
                        Some(vec)
                    }
                } else {
                    None
                };
                // TODO update user identification methods
                let competition_results = competition_results(user_data.license_number).await;
                let license_number = Some(user_data.license_number);
                let gender = Some(user_data.gender);
                let license_type = latest_license.as_ref().map(|it| it.product.product);
                let latest_license_season = latest_license.as_ref().map(|it| it.season.season);
                let medical_certificate_status = latest_license
                    .as_ref()
                    .map(|it| it.medical_certificate_status);
                if metadata.license_number != license_number
                    || metadata.gender != gender
                    || metadata.license_type != license_type
                    || metadata.latest_license_season != latest_license_season
                    || metadata.latest_structure != latest_structure
                    || metadata.medical_certificate_status != metadata.medical_certificate_status
                    || metadata.address != address
                    || metadata.emergency_contacts != emergency_contacts
                    || metadata.competition_results != competition_results
                {
                    modified = true;
                    user.metadata = Some(
                        serde_json::to_value(Metadata {
                            license_number,
                            gender,
                            license_type,
                            latest_license_season,
                            latest_structure,
                            medical_certificate_status,
                            address,
                            emergency_contacts,
                            competition_results,
                            ..metadata
                        })
                        .or_else(|err| {
                            warn!("failed to serialize metadata:\n{err:?}");
                            Err("failed to serialize metadata".to_string())
                        })?,
                    );
                }
                if modified {
                    Snapshot::set_and_return_before_update(key.as_str(), &user)
                        .await
                        .ok_or("failed to update user".to_string())?;
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiered_server::store::snapshot;
    use tiered_server::user::ensure_admin_users_exist;

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

    #[tokio::test]
    #[ignore]
    async fn test_add_missing_users() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .without_time()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
            ))
            .init();
        let token = update_myffme_bearer_token(0, None)
            .await
            .expect("failed to get bearer token");
        ensure_admin_users_exist(&snapshot()).await.unwrap();
        add_missing_users(&snapshot(), false).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_update_users() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .without_time()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
            ))
            .init();
        let token = update_myffme_bearer_token(0, None)
            .await
            .expect("failed to get bearer token");
        update_users_metadata(&snapshot(), false).await.unwrap();
    }
}

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
use crate::mycompet::results::competition_results;
use crate::myffme::member::{members_by_ids, members_by_structure, Member};
use crate::user::Metadata;
use pinboard::Pinboard;
use reqwest::header::{HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::norm::{normalize_first_name, normalize_last_name};
use tiered_server::store::Snapshot;
use tiered_server::user::{Email, IdentificationMethod, User};
use tracing::{info, warn};

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

pub(crate) async fn add_missing_users(
    snapshot: &Snapshot,
    season: Option<u16>,
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
    let members = members_by_structure(*STRUCTURE_ID, season)
        .await
        .ok_or("failed to get members")?;
    info!("members: {}", members.len());
    if let Some(output) = output.as_mut() {
        let _ = writeln!(output, "members: {}", members.len());
    }
    for member in members {
        if lookup.contains_key(
            member
                .metadata
                .myffme_user_id
                .as_ref()
                .ok_or("missing myffme user id")?,
        ) {
            continue;
        }
        let Member {
            first_name,
            last_name,
            email,
            dob,
            metadata,
            ..
        } = member;
        let identification = IdentificationMethod::Email(Email::from(email));
        let normalized_first_name = normalize_first_name(first_name.as_str());
        let normalized_last_name = normalize_last_name(last_name.as_str());
        let last = existing_users
            .iter()
            .filter(|&it| {
                it.date_of_birth == dob
                    && it.normalized_first_name == normalized_first_name
                    && it.normalized_last_name == normalized_last_name
            })
            .enumerate()
            .last();
        if let Some((i, it)) = last {
            if i == 0 {
                if let Some(ref myffme_user_id) = it.metadata.as_ref().and_then(|value| {
                    Metadata::deserialize(value)
                        .ok()
                        .and_then(|it| it.myffme_user_id)
                }) {
                    return Err(format!(
                        "user already exists {first_name} {last_name}: {myffme_user_id}"
                    ));
                }
            } else {
                return Err(format!("multiple users found for {first_name} {last_name}"));
            }
        }
        let last = last.map(|(_, it)| it);
        if let Some(user) = last {
            info!("updating {first_name} {last_name}");
            if let Some(output) = output.as_mut() {
                let _ = writeln!(output, "updating {first_name} {last_name}");
            }
            let metadata =
                serde_json::to_value(&metadata).map_err(|_| "failed to serialize metadata")?;
            let mut user = user.clone();
            user.metadata = Some(metadata);
            let key = format!("acc/{}", user.id);
            Snapshot::set_and_return_before_update(key.as_str(), &user)
                .await
                .ok_or("failed to update user".to_string())?;
        } else {
            let id = User::new_id(0);
            let key = format!("acc/{id}");
            info!("adding {first_name} {last_name}");
            if let Some(output) = output.as_mut() {
                let _ = writeln!(output, "adding {first_name} {last_name}");
            }
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
    let user_ids = entries
        .iter()
        .filter_map(|(_, User { metadata, .. })| {
            metadata.as_ref().and_then(|it| {
                serde_json::from_value::<Metadata>(it.clone())
                    .ok()
                    .and_then(|it| it.myffme_user_id.map(|it| it.to_string()))
            })
        })
        .collect::<Vec<_>>();
    if user_ids.is_empty() {
        return Ok(output);
    }
    let members = members_by_ids(
        &user_ids.iter().map(|it| it.as_str()).collect::<Vec<_>>(),
        None,
    )
    .await
    .ok_or("failed to get members".to_string())?;
    info!("members: {}", members.len());
    if let Some(output) = output.as_mut() {
        let _ = writeln!(output, "members: {}", members.len());
    }
    let mut members_metadata = BTreeMap::new();
    let mut members_competition_results = BTreeMap::new();
    for member in members {
        let metadata = member.metadata;
        if let Some(ffme_user_id) = metadata.myffme_user_id.clone() {
            if let Some(license_number) = metadata.license_number {
                if let Some(competition_results) = competition_results(license_number).await {
                    members_competition_results.insert(ffme_user_id.clone(), competition_results);
                }
            }
            members_metadata.insert(ffme_user_id, metadata);
        }
    }
    info!("competition results: {}", members_competition_results.len());
    if let Some(output) = output.as_mut() {
        let _ = writeln!(
            output,
            "competition results: {}",
            members_competition_results.len()
        );
    }
    let mut updates = Vec::new();
    for (key, mut user) in entries {
        if let Some(metadata) = user.metadata {
            let mut metadata = serde_json::from_value::<Metadata>(metadata)
                .map_err(|_| "failed to parse metadata".to_string())?;
            if let Some(ffme_user_id) = metadata.myffme_user_id.as_ref() {
                if let Some(found) = members_metadata.remove(ffme_user_id) {
                    let mut changed = false;
                    if metadata.insee != found.insee {
                        metadata.insee = found.insee;
                        changed = true;
                    }
                    // if metadata.zip_code != found.zip_code {
                    //     metadata.zip_code = found.zip_code;
                    //     changed = true;
                    // }
                    // if metadata.city != found.city {
                    //     metadata.city = found.city;
                    //     changed = true;
                    // }
                    if metadata.latest_license_season != found.latest_license_season {
                        metadata.latest_license_season = found.latest_license_season;
                        changed = true;
                    }
                    if metadata.medical_certificate_status.as_ref()
                        != found.medical_certificate_status.as_ref()
                    {
                        metadata.medical_certificate_status = found.medical_certificate_status;
                        changed = true;
                    }
                    if metadata.latest_structure.as_ref().map(|it| it.id)
                        != found.latest_structure.as_ref().map(|it| it.id)
                    {
                        metadata.latest_structure = found.latest_structure;
                        changed = true;
                    }
                    let competition_results = members_competition_results.remove(ffme_user_id);
                    if let Some(competition_results) = competition_results {
                        if metadata
                            .competition_results
                            .as_ref()
                            .map(|it| it.len())
                            .unwrap_or(0)
                            != competition_results.len()
                        {
                            metadata.competition_results = Some(competition_results);
                            changed = true;
                        }
                    }
                    if changed {
                        info!("updating {} {}", user.first_name, user.last_name);
                        let metadata = serde_json::to_value(&metadata)
                            .map_err(|_| "failed to serialize metadata".to_string())?;
                        user.metadata = Some(metadata);
                        updates.push((key, user));
                    }
                }
            }
        }
    }
    for (key, user) in updates {
        Snapshot::set_and_return_before_update(&key, &user)
            .await
            .ok_or("failed to update user".to_string())?;
    }
    Ok(output)
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

use crate::address::City;
use crate::http_client::json_client;
use crate::order::{InsuranceLevel, InsuranceOption};
use crate::season::current_season;
use crate::user::LicenseType::NonPracticing;
use crate::user::{Gender, LicenseType, MedicalCertificateStatus, Metadata, Structure};
use hyper::header::{AUTHORIZATION, ORIGIN, REFERER};
use hyper::http::{HeaderName, HeaderValue};
use pinboard::Pinboard;
use reqwest::{Response, Url};
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::str::from_utf8;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::norm::{normalize_first_name, normalize_last_name};
#[allow(unused_imports)]
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

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

#[derive(Deserialize, Serialize)]
pub struct Member {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub dob: u32,
    pub metadata: Metadata,
}

pub async fn member_by_name_and_dob(
    first_name: &str,
    last_name: &str,
    dob: u32,
) -> Option<Vec<Member>> {
    let response = users_response_by_dob(dob).await?;
    let mut results = users_response_to_members(response, current_season(None)).await?;
    let normalized_first_name = normalize_first_name(first_name);
    let normalized_last_name = normalize_last_name(last_name);

    if results.len() > 1 {
        let found = results
            .iter()
            .filter_map(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    if normalize_first_name(&it.first_name) == normalized_first_name {
                        return Some(license_number);
                    }
                }
                None
            })
            .collect::<BTreeSet<_>>();
        if !found.is_empty() {
            results.retain(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    found.contains(&license_number)
                } else {
                    false
                }
            });
        }
        let found = results
            .iter()
            .filter_map(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    if normalize_first_name(&it.last_name) == normalized_last_name {
                        return Some(license_number);
                    }
                }
                None
            })
            .collect::<BTreeSet<_>>();
        if !found.is_empty() {
            results.retain(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    found.contains(&license_number)
                } else {
                    false
                }
            });
        }
    }
    Some(results)
}

pub async fn member_by_license_number(license_number: u32) -> Option<Member> {
    let response = users_response_by_license_numbers(&[license_number]).await?;
    let mut iter = users_response_to_members(response, current_season(None))
        .await?
        .into_iter();
    let first = iter.next()?;
    if iter.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub async fn members_by_structure(structure_id: u32, season: Option<u16>) -> Option<Vec<Member>> {
    let season = season.unwrap_or_else(|| current_season(None));
    let response = users_response_by_structure(structure_id).await?;
    users_response_to_members(response, season).await
}

pub async fn members_by_ids(ids: &[&str], season: Option<u16>) -> Option<Vec<Member>> {
    let season = season.unwrap_or_else(|| current_season(None));
    let response = users_response_by_ids(ids).await?;
    users_response_to_members(response, season).await
}

pub async fn licensees(structure_id: u32, season: u16) -> Option<Vec<Member>> {
    let licenses = structure_licenses(structure_id, season).await?;
    let user_ids = licenses.keys().map(|it| it.as_str()).collect::<Vec<_>>();
    let response = users_response_by_ids(&user_ids).await?;
    #[cfg(test)]
    let users = {
        println!("users");
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".licensees_{structure_id}_{season}.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|err| {
                eprintln!("{err:?}");
                err
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let users = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    let addresses = user_addresses(&user_ids).await?;
    let medical_certificates = user_medical_certificates(&user_ids, season).await?;
    let health_questionnaires = user_health_questionnaires(&user_ids, season).await?;
    let structure_ids = licenses
        .values()
        .map(|it| it.structure_id)
        .collect::<Vec<_>>();
    let structures = structures_by_ids(&structure_ids).await?;
    Some(members(
        users,
        licenses,
        addresses,
        medical_certificates,
        health_questionnaires,
        structures,
    ))
}

async fn users_response_by_license_numbers(license_numbers: &[u32]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByLicenseNumbers",
            "query": GRAPHQL_GET_USERS_BY_LICENSE_NUMBERS,
            "variables": {
                "license_numbers": license_numbers
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_by_ids(ids: &[&str]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByIds",
            "query": GRAPHQL_GET_USERS_BY_IDS,
            "variables": {
                "ids": ids
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_by_structure(structure_id: u32) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByStructureId",
            "query": GRAPHQL_GET_USERS_BY_STRUCTURE_ID,
            "variables": {
                "id": structure_id
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_by_dob(dob: u32) -> Option<Response> {
    let s = dob.to_string();
    let dob = s.as_bytes();
    let yyyy = from_utf8(&dob[..4]).unwrap();
    let mm = from_utf8(&dob[4..6]).unwrap();
    let dd = from_utf8(&dob[6..]).unwrap();
    let dob = format!("{yyyy}-{mm}-{dd}");
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByDateOfBirth",
            "query": GRAPHQL_GET_USERS_BY_DATE_OF_BIRTH,
            "variables": {
                "dob": dob
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_to_members(response: Response, season: u16) -> Option<Vec<Member>> {
    #[cfg(test)]
    let users = {
        println!("users");
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".users.json";
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|err| {
                eprintln!("{err:?}");
                err
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let users = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    let user_ids = users.iter().map(|it| it.id.as_str()).collect::<Vec<_>>();
    let licenses = user_licenses(&user_ids, season).await?;
    let addresses = user_addresses(&user_ids).await?;
    let medical_certificates = user_medical_certificates(&user_ids, season).await?;
    let health_questionnaires = user_health_questionnaires(&user_ids, season).await?;
    let structure_ids = licenses
        .values()
        .map(|it| it.structure_id)
        .collect::<Vec<_>>();
    let structures = structures_by_ids(&structure_ids).await?;
    Some(members(
        users,
        licenses,
        addresses,
        medical_certificates,
        health_questionnaires,
        structures,
    ))
}

fn members(
    users: Vec<User>,
    mut licenses: BTreeMap<String, License>,
    mut addresses: BTreeMap<String, Address>,
    mut medical_certificates: BTreeMap<String, Document>,
    mut health_questionnaires: BTreeMap<String, Document>,
    structures: BTreeMap<u32, Structure>,
) -> Vec<Member> {
    users
        .into_iter()
        .map(|it| {
            let license = licenses.remove(&it.id);
            let address = addresses.remove(&it.id);
            let latest_structure = license
                .as_ref()
                .and_then(|it| structures.get(&it.structure_id).cloned());
            let latest_license_season = license.as_ref().map(|it| it.season);
            let license_type = if it.non_practicing.unwrap_or(false) {
                Some(NonPracticing)
            } else {
                license.and_then(|it| it.license_type)
            };
            let health_questionnaire = health_questionnaires.remove(&it.id).and_then(|it| {
                if let Some(season) = latest_license_season {
                    if it.season == season { Some(it) } else { None }
                } else {
                    None
                }
            });
            let medical_certificate = medical_certificates.remove(&it.id).unwrap_or(Document {
                user_id: None,
                season: 0,
                category: 5,
            });
            let medical_certificate_status =
                latest_license_season.map(|season| match medical_certificate.category {
                    5 => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Recreational
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                if medical_certificate.season + 3 > season {
                                    MedicalCertificateStatus::Recreational
                                } else {
                                    MedicalCertificateStatus::HealthQuestionnaire
                                }
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                    9 => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Competition
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                if medical_certificate.season + 3 > season {
                                    MedicalCertificateStatus::Competition
                                } else {
                                    MedicalCertificateStatus::HealthQuestionnaire
                                }
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                    _ => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Recreational
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                MedicalCertificateStatus::HealthQuestionnaire
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                });
            let (insee, city, zip_code) = address
                .map(|it| (it.insee, it.city, it.zip_code))
                .unwrap_or((None, None, None));
            Member {
                first_name: it.first_name,
                last_name: it.last_name,
                email: it.email.unwrap_or_else(|| it.alt_email.unwrap()),
                dob: it.dob,
                metadata: Metadata {
                    myffme_user_id: Some(it.id),
                    license_number: Some(it.license_number),
                    gender: Some(it.gender),
                    insee,
                    city,
                    zip_code,
                    license_type,
                    medical_certificate_status,
                    latest_license_season,
                    latest_structure,
                    ..Default::default()
                },
            }
        })
        .collect()
}

async fn user_medical_certificates(
    ids: &[&str],
    season: u16,
) -> Option<BTreeMap<String, Document>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getMedicalCertificatesByUserIds",
            "query": GRAPHQL_GET_MEDICAL_CERTIFICATES_BY_USER_IDS,
            "variables": {
                "ids": ids,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client
        .execute(request)
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?;
    #[derive(Deserialize)]
    struct DocumentList {
        list: Vec<Document>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: DocumentList,
    }
    #[cfg(test)]
    let documents = {
        println!("medical certificates");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".medical_certificates.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let documents = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        documents
            .into_iter()
            .map(|mut document| {
                let id = document.user_id.take().unwrap();
                (id, document)
            })
            .collect(),
    )
}

async fn user_health_questionnaires(
    ids: &[&str],
    season: u16,
) -> Option<BTreeMap<String, Document>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getHealthQuestionnairesByUserIds",
            "query": GRAPHQL_GET_HEALTH_QUESTIONNAIRES_BY_USER_IDS,
            "variables": {
                "ids": ids,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct DocumentList {
        list: Vec<Document>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: DocumentList,
    }
    #[cfg(test)]
    let documents = {
        println!("health questionnaires");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".health_questionnaires.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let documents = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        documents
            .into_iter()
            .map(|mut document| {
                let id = document.user_id.take().unwrap();
                (id, document)
            })
            .collect(),
    )
}

pub(crate) async fn user_address(id: &str) -> Option<Address> {
    user_addresses([id].as_slice())
        .await
        .and_then(|mut it| it.remove(id))
}

async fn user_addresses(ids: &[&str]) -> Option<BTreeMap<String, Address>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getAddressesByUserIds",
            "query": GRAPHQL_GET_ADDRESSES_BY_USER_IDS,
            "variables": {
                "ids": ids,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct AddressList {
        list: Vec<Address>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: AddressList,
    }
    #[cfg(test)]
    let addresses = {
        println!("addresses");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".addresses.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let addresses = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        addresses
            .into_iter()
            .map(|mut address| {
                let id = address.user_id.take().unwrap();
                (id, address)
            })
            .collect(),
    )
}

async fn user_licenses(ids: &[&str], season: u16) -> Option<BTreeMap<String, License>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getLicensesByUserIds",
            "query": GRAPHQL_GET_LICENSES_BY_USER_IDS,
            "variables": {
                "ids": ids,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct LicenseList {
        list: Vec<License>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: LicenseList,
    }
    #[cfg(test)]
    let licenses = {
        println!("licenses");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".licenses.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let licenses = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        licenses
            .into_iter()
            .map(|mut license| {
                let id = license.user_id.take().unwrap();
                (id, license)
            })
            .collect(),
    )
}

async fn structure_licenses(structure_id: u32, season: u16) -> Option<BTreeMap<String, License>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getLicensesByStructureIdAndSeason",
            "query": GRAPHQL_GET_LICENSES_BY_STRUCTURE_ID_AND_SEASON,
            "variables": {
                "structure_id": structure_id,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct LicenseList {
        list: Vec<License>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: LicenseList,
    }
    #[cfg(test)]
    let licenses = {
        println!("licenses");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".licenses.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let licenses = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        licenses
            .into_iter()
            .map(|mut license| {
                let id = license.user_id.take().unwrap();
                (id, license)
            })
            .collect(),
    )
}

async fn structures_by_ids(ids: &[u32]) -> Option<BTreeMap<u32, Structure>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getStructuresByIds",
            "query": GRAPHQL_GET_STRUCTURES_BY_IDS,
            "variables": {
                "ids": ids,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct StructureList {
        list: Vec<Structure>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: StructureList,
    }
    #[cfg(test)]
    let structures = {
        println!("structures");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".structures.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let structures = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        structures
            .into_iter()
            .map(|structure| {
                let id = structure.id;
                (id, structure)
            })
            .collect(),
    )
}

#[derive(Deserialize)]
struct StructureHierarchy {
    id: u32,
    department_structure_id: u32,
    region_structure_id: u32,
    national_structure_id: u32,
}

async fn structure_hierarchy_by_id(id: u32) -> Option<StructureHierarchy> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getStructuresByIds",
            "query": GRAPHQL_GET_STRUCTURES_BY_IDS,
            "variables": {
                "ids": [id],
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct StructureList {
        list: Vec<StructureHierarchy>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: StructureList,
    }
    #[cfg(test)]
    let structure_hierarchy = {
        println!("structure");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".structure.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
            .into_iter()
            .next()?
    };
    #[cfg(not(test))]
    let structure_hierarchy = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list
        .into_iter()
        .next()?;
    Some(structure_hierarchy)
}

#[derive(Deserialize)]
struct Product {
    id: String,
    #[serde(deserialize_with = "deserialize_license_type")]
    license_type: Option<LicenseType>,
}

async fn products() -> Option<Vec<Product>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getProducts",
            "query": GRAPHQL_GET_PRODUCTS,
            "variables": {}
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct ProductList {
        list: Vec<Product>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: ProductList,
    }
    #[cfg(test)]
    let products = {
        println!("products");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".products.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let products = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(products)
}

#[derive(Deserialize)]
struct InsuranceLevelOption {
    id: String,
    #[serde(deserialize_with = "deserialize_insurance_level")]
    level: Option<InsuranceLevel>,
}

#[derive(Deserialize)]
struct InsuranceOptionOption {
    id: String,
    #[serde(deserialize_with = "deserialize_insurance_option")]
    option: Option<InsuranceOption>,
}

async fn options() -> Option<(Vec<InsuranceLevelOption>, Vec<InsuranceOptionOption>)> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getOptions",
            "query": GRAPHQL_GET_OPTIONS,
            "variables": {}
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct OptionList {
        levels: Vec<InsuranceLevelOption>,
        options: Vec<InsuranceOptionOption>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: OptionList,
    }
    #[cfg(test)]
    let options = {
        println!("options");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".options.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
    };
    #[cfg(not(test))]
    let options = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data;
    Some((options.levels, options.options))
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct LicenseFees {
    pub federal_fee_in_cents: u16,
    pub regional_fee_in_cents: u16,
    pub department_fee_in_cents: u16,
}

pub async fn prices(
    season: Option<u16>,
) -> Option<(
    BTreeMap<LicenseType, LicenseFees>,
    BTreeMap<InsuranceLevel, u16>,
    BTreeMap<InsuranceOption, u16>,
)> {
    let StructureHierarchy {
        department_structure_id,
        region_structure_id,
        national_structure_id,
        ..
    } = structure_hierarchy_by_id(*STRUCTURE_ID).await?;
    let products = products().await?;
    let (levels, options) = options().await?;
    let mut levels = levels
        .into_iter()
        .filter_map(|it| it.level.map(|level| (it.id, level)))
        .collect::<BTreeMap<_, _>>();
    let mut options = options
        .into_iter()
        .filter_map(|it| it.option.map(|option| (it.id, option)))
        .collect::<BTreeMap<_, _>>();
    let product_ids = products.iter().map(|it| it.id.as_str()).collect::<Vec<_>>();
    let level_ids = levels.keys().collect::<Vec<_>>();
    let option_ids = options.keys().collect::<Vec<_>>();
    let season = season.unwrap_or(current_season(None));
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getPrices",
            "query": GRAPHQL_GET_PRICES,
            "variables": {
                "products": product_ids,
                "levels": level_ids,
                "options": option_ids,
                "department_structure_id": department_structure_id,
                "region_structure_id": region_structure_id,
                "national_structure_id": national_structure_id,
                "season": season
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct Product {
        product_id: String,
        structure_id: u32,
        price_in_cents: u16,
    }
    #[derive(Deserialize)]
    struct LevelOrOption {
        option_id: String,
        price_in_cents: u16,
    }

    #[derive(Deserialize)]
    struct PriceList {
        products: Vec<Product>,
        levels: Vec<LevelOrOption>,
        options: Vec<LevelOrOption>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: PriceList,
    }
    #[cfg(test)]
    let PriceList {
        products: product_list,
        levels: level_list,
        options: option_list,
    } = {
        println!("prices");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".prices.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
    };
    #[cfg(not(test))]
    let PriceList {
        products: product_list,
        levels: level_list,
        options: option_list,
    } = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data;
    let mut license_prices = BTreeMap::new();
    for price in product_list.into_iter() {
        if let Some(license_type) = products
            .iter()
            .find(|it| it.id == price.product_id)
            .and_then(|it| it.license_type)
        {
            let fees: &mut LicenseFees = license_prices.entry(license_type).or_default();
            if price.structure_id == department_structure_id {
                fees.department_fee_in_cents = price.price_in_cents;
            } else if price.structure_id == region_structure_id {
                fees.regional_fee_in_cents = price.price_in_cents;
            } else if price.structure_id == national_structure_id {
                fees.federal_fee_in_cents = price.price_in_cents;
            }
        }
    }
    let mut level_prices = BTreeMap::new();
    for price in level_list.into_iter() {
        if let Some(level) = levels.remove(&price.option_id) {
            level_prices.insert(level, price.price_in_cents);
        }
    }
    let mut option_prices = BTreeMap::new();
    for price in option_list.into_iter() {
        if let Some(option) = options.remove(&price.option_id) {
            option_prices.insert(option, price.price_in_cents);
        }
    }
    Some((license_prices, level_prices, option_prices))
}

#[derive(Deserialize)]
struct Document {
    pub user_id: Option<String>,
    pub season: u16,
    pub category: u8,
}

#[derive(Deserialize)]
struct User {
    pub id: String,
    #[serde(deserialize_with = "deserialize_gender")]
    pub gender: Gender,
    pub first_name: String,
    pub last_name: String,
    // pub birth_name: Option<String>,
    #[serde(deserialize_with = "deserialize_date")]
    pub dob: u32,
    pub email: Option<String>,
    pub alt_email: Option<String>,
    // pub phone_number: Option<String>,
    // pub alt_phone_number: Option<String>,
    pub license_number: u32,
    pub non_practicing: Option<bool>,
}
#[derive(Deserialize)]
struct UserList {
    list: Vec<User>,
}
#[derive(Deserialize)]
struct GraphqlResponse {
    data: UserList,
}

pub async fn update_email(user_id: &str, email: &str, alt_email: Option<&str>) -> Option<()> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "updateEmail",
            "query": GRAPHQL_UPDATE_EMAIL,
            "variables": {
                "user_id": user_id,
                "email": email,
                "alt_email": alt_email,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    let success = response.status().is_success();
    if success {
        #[derive(Deserialize)]
        struct UserId {
            id: String,
        }
        #[derive(Deserialize)]
        struct MutationResult {
            result: Option<UserId>,
        }
        #[derive(Deserialize)]
        struct GraphqlResponse {
            data: MutationResult,
        }
        #[cfg(test)]
        let id = {
            println!("POST {}", url.as_str());
            println!("{}", response.status());
            let text = response.text().await.ok()?;
            let file_name = format!(".update_email_{user_id}.json");
            tokio::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&file_name)
                .await
                .ok()?
                .write_all(text.as_bytes())
                .await
                .unwrap();
            serde_json::from_str::<GraphqlResponse>(&text)
                .map_err(|err| {
                    eprintln!("{err:?}");
                    err
                })
                .ok()?
                .data
                .result
                .map(|it| it.id)
        };
        #[cfg(not(test))]
        let id = response
            .json::<GraphqlResponse>()
            .await
            .map_err(|err| {
                warn!("{err:?}");
                err
            })
            .ok()?
            .data
            .result
            .map(|it| it.id);
        if let Some(ref id) = id {
            if id == user_id { Some(()) } else { None }
        } else {
            None
        }
    } else {
        None
    }
}

pub async fn update_address(
    user_id: &str,
    zip_code: &str,
    city: &City,
    line1: Option<&str>,
    country_id: Option<u16>,
) -> Option<()> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "updateAddress",
            "query": GRAPHQL_UPDATE_ADDRESS_CITY,
            "variables": {
                "id": user_id,
                "city": city.name,
                "zip": zip_code,
                "insee": city.insee
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    let success = response.status().is_success();
    if success {
        #[derive(Deserialize)]
        struct AffectedRows {
            affected_rows: u16,
        }
        #[derive(Deserialize)]
        struct MutationResult {
            result: AffectedRows,
        }
        #[derive(Deserialize)]
        struct GraphqlResponse {
            data: MutationResult,
        }
        #[cfg(test)]
        let affected_rows = {
            println!("POST {}", url.as_str());
            println!("{}", response.status());
            let text = response.text().await.ok()?;
            let file_name = format!(".update_address_{user_id}.json");
            tokio::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&file_name)
                .await
                .ok()?
                .write_all(text.as_bytes())
                .await
                .unwrap();
            serde_json::from_str::<GraphqlResponse>(&text)
                .map_err(|err| {
                    eprintln!("{err:?}");
                    err
                })
                .ok()?
                .data
                .result
                .affected_rows
        };
        #[cfg(not(test))]
        let affected_rows = response
            .json::<GraphqlResponse>()
            .await
            .map_err(|err| {
                warn!("{err:?}");
                err
            })
            .ok()?
            .data
            .result
            .affected_rows;
        if affected_rows > 0 {
            Some(())
        } else {
            let client = json_client();
            let request = client
                .post(url.as_str())
                .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
                .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
                .header(X_HASURA_ROLE, ADMIN)
                .header(
                    AUTHORIZATION,
                    MYFFME_AUTHORIZATION
                        .get_ref()
                        .map(|it| it.bearer_token.clone())?,
                )
                .json(&json!({
                    "operationName": "insertAddress",
                    "query": GRAPHQL_INSERT_ADDRESS_CITY,
                    "variables": {
                        "id": user_id,
                        "city": city.name,
                        "zip": zip_code,
                        "insee": city.insee,
                        "line1": line1.unwrap_or_default(),
                        "country_id": country_id.unwrap_or(75)
                    }
                }))
                .build()
                .ok()?;
            let response = client.execute(request).await.ok()?;
            let success = response.status().is_success();
            #[cfg(test)]
            {
                println!("POST {}", url.as_str());
                println!("{}", response.status());
                let text = response.text().await.ok()?;
                let file_name = format!(".insert_address_{user_id}.json");
                tokio::fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .create(true)
                    .open(&file_name)
                    .await
                    .ok()?
                    .write_all(text.as_bytes())
                    .await
                    .unwrap();
            }
            if success { Some(()) } else { None }
        }
    } else {
        None
    }
}

impl TryFrom<&str> for LicenseType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "adult" | "licence_adulte" | "ab229bd0-53c7-4c8c-83d1-bade2cbb5fcc" => {
                Ok(LicenseType::Adult)
            }
            "non_member_adult" | "hors_club_adulte" | "8dd8c63f-a9da-4237-aec9-74f905fb2b37" => {
                Ok(LicenseType::NonMemberAdult)
            }
            "child" | "licence_jeune" | "09fd57d3-0f38-407d-95b5-08d3e8369297" => {
                Ok(LicenseType::Child)
            }
            "non_member_child" | "hors_club_jeune" | "46786452-7ca2-4dc1-a15d-effb3f7e69b0" => {
                Ok(LicenseType::NonMemberChild)
            }
            "family" | "licence_famille" | "865d950e-9825-49f3-858b-ca1a776734b3" => {
                Ok(LicenseType::Family)
            }
            "non_practicing" => Ok(LicenseType::NonPracticing),
            other => Err(format!("unknown license type: {other}")),
        }
    }
}

impl TryFrom<&str> for InsuranceLevel {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "rc" | "Rc" | "RC" | "8e1b2635-a76a-40a4-a278-2cd6768d03c0" => Ok(InsuranceLevel::RC),
            "base" | "Base" | "4061064e-4d0a-4c49-9c66-109960a0437a" => Ok(InsuranceLevel::Base),
            "base_plus" | "BasePlus" | "a3a2d318-c8a5-410b-ac9d-1f07c1d69bdc" => {
                Ok(InsuranceLevel::BasePlus)
            }
            "base_plus_plus" | "BasePlusPlus" | "902fb734-a182-419a-af61-008b8bff3a4a" => {
                Ok(InsuranceLevel::BasePlusPlus)
            }
            other => Err(format!("unknown insurance level: {other}")),
        }
    }
}

impl TryFrom<&str> for InsuranceOption {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "vtt" | "MountainBike" | "mountain_bike" | "5e6eb7ec-7dc6-445b-ab50-9b45cb202f1e" => {
                Ok(InsuranceOption::MountainBike)
            }
            "ski_piste" | "Ski" | "ski" | "92e7eebe-71cd-4258-b178-141587374b81" => {
                Ok(InsuranceOption::Ski)
            }
            "slackline_highline"
            | "SlacklineAndHighline"
            | "slackline_and_highline"
            | "dae0654d-977c-46c5-8f48-63de2d127efd" => Ok(InsuranceOption::SlacklineAndHighline),
            "trail" | "TrialRunning" | "trial_running" | "d9c13113-70eb-4e04-a265-aba8f8ea7e8b" => {
                Ok(InsuranceOption::TrailRunning)
            }
            other => Err(format!("unknown insurance option: {other}")),
        }
    }
}

#[derive(Deserialize, Serialize, Default)]
pub(crate) struct Address {
    #[serde(skip_serializing)]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

#[derive(Deserialize)]
struct License {
    pub user_id: Option<String>,
    pub season: u16,
    // #[serde(deserialize_with = "deserialize_license_number")]
    // pub license_number: u32,
    pub structure_id: u32,
    #[serde(rename = "product_id", deserialize_with = "deserialize_license_type")]
    pub license_type: Option<LicenseType>,
}

fn deserialize_date<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    let date = s
        .split('T')
        .next()
        .ok_or_else(|| Error::custom("invalid date"))?;
    let mut split = date.split('-');
    let yyyy = split.next().ok_or_else(|| Error::custom("invalid date"))?;
    let mm = split.next().ok_or_else(|| Error::custom("invalid date"))?;
    let dd = split.next().ok_or_else(|| Error::custom("invalid date"))?;
    format!("{yyyy}{mm}{dd}").parse().map_err(Error::custom)
}

fn deserialize_gender<'de, D>(deserializer: D) -> Result<Gender, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let n: u8 = serde::Deserialize::deserialize(deserializer)?;
    match n {
        0 => Ok(Gender::Female),
        1 => Ok(Gender::Male),
        _ => Err(Error::custom("invalid gender")),
    }
}

fn deserialize_license_type<'de, D>(deserializer: D) -> Result<Option<LicenseType>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
}

fn deserialize_insurance_level<'de, D>(deserializer: D) -> Result<Option<InsuranceLevel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
}

fn deserialize_insurance_option<'de, D>(
    deserializer: D,
) -> Result<Option<InsuranceOption>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
}

const X_HASURA_ROLE: HeaderName = HeaderName::from_static("x-hasura-role");
const ADMIN: HeaderValue = HeaderValue::from_static("admin");

const GRAPHQL_GET_USERS_BY_IDS: &str = "\
    query getUsersByIds(
        $ids: [uuid!]!
    ) {
        list: UTI_Utilisateurs(
            where: { id: { _in: $ids } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_LICENSE_NUMBERS: &str = "\
    query getUsersByLicenseNumbers(
        $license_numbers: [bigint!]!
    ) {
        list: UTI_Utilisateurs(
            where: { LicenceNumero: { _in: $license_numbers } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_DATE_OF_BIRTH: &str = "\
    query getUsersByDateOfBirth(
        $dob: date!
    ) {
        list: UTI_Utilisateurs(
            where: { DN_DateNaissance: { _eq: $dob } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_STRUCTURE_ID: &str = "\
    query getUsersByStructureId(
        $id: Int!
    ) {
        list: UTI_Utilisateurs(
            where: { STR_StructureUtilisateurs: { ID_Structure: { _eq: $id} } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_ADDRESSES_BY_USER_IDS: &str = "\
    query getAddressesByUserIds(
        $ids: [uuid!]!
    ) {
        list: ADR_Adresse(
            where: { ID_Utilisateur: { _in: $ids } }
            order_by: [ { ID_Utilisateur: asc }, { Z_DateModification: desc } ]
            distinct_on: [ ID_Utilisateur ]
        ) {
            user_id: ID_Utilisateur
            line1: Adresse1
            line2: Adresse2
            insee: CodeInsee
            zip_code: CodePostal,
            city: Ville
        }
    }\
";

const GRAPHQL_GET_LICENSES_BY_USER_IDS: &str = "\
    query getLicensesByUserIds(
        $ids: [uuid!]!
        $season: Int!
    ) {
        list: licence(
            where: { user_id: { _in: $ids }, season_id: { _lte: $season } }
            order_by: [ { user_id: asc }, { season_id: desc_nulls_last } ]
            distinct_on: user_id
        ) {
            product_id
            non_practicing
            structure_id
            status
            user_id
            season: season_id
        }
    }\
";

const GRAPHQL_GET_LICENSES_BY_STRUCTURE_ID_AND_SEASON: &str = "\
    query getLicensesByStructureIdAndSeason(
        $structure_id: Int!
        $season: Int!
    ) {
        list: licence(
            where: { structure_id: { _eq: $structure_id }, season_id: { _eq: $season } }
        ) {
            product_id
            non_practicing
            structure_id
            status
            user_id
            season: season_id
        }
    }\
";

const GRAPHQL_GET_MEDICAL_CERTIFICATES_BY_USER_IDS: &str = "\
    query getMedicalCertificatesByUserIds(
        $ids: [uuid!]!
        $season: Int!
    ) {
        list: DOC_Document(
            distinct_on: ID_Utilisateur
            order_by: [ { ID_Utilisateur: asc }, { ID_Saison: desc_nulls_last } ]
            where: {
                ID_Utilisateur: { _in: $ids }
                EST_DocumentValide: { _eq: true }
                EST_Actif: { _eq: true }
                ID_Type_Document: { _in: [ 5, 6, 7, 9 ] }
                ID_Saison: { _lte: $season }
            }
        ) {
            user_id: ID_Utilisateur,
            season: ID_Saison,
            status,
            category: ID_Type_Document
        }
    }\
";

const GRAPHQL_GET_HEALTH_QUESTIONNAIRES_BY_USER_IDS: &str = "\
    query getHealthQuestionnairesByUserIds(
        $ids: [uuid!]!
        $season: Int!
    ) {
        list: DOC_Document(
            distinct_on: ID_Utilisateur
            order_by: [ { ID_Utilisateur: asc }, { ID_Saison: desc_nulls_last } ]
            where: {
                ID_Utilisateur: { _in: $ids }
                EST_DocumentValide: { _eq: true }
                EST_Actif: { _eq: true }
                ID_Type_Document: { _in: [ 60 ] }
                ID_Saison: { _lte: $season }
            }
        ) {
            user_id: ID_Utilisateur,
            season: ID_Saison,
            status,
            category: ID_Type_Document
        }
    }\
";

const GRAPHQL_GET_STRUCTURES_BY_IDS: &str = "\
    query getStructuresByIds($ids: [Int!]!) {
        list: structure(
            where: { id: { _in: $ids } }
        ) {
            id
            code: federal_code
            name: label
            department: department_id
            department_structure_id: ct_id
            region_structure_id: ligue_id
            national_structure_id: ffme_id
        }
    }\
";

// ProductCategory {
//     id: "d5b8f23e-cd8e-4179-ac21-0b6f150820f4",
//     slug: "licence"
// }
const GRAPHQL_GET_PRODUCTS: &str = "\
    query getProducts {
        list: product(
            where: {
                product_categorie_id: { _eq: \"d5b8f23e-cd8e-4179-ac21-0b6f150820f4\" }
            }
        ) {
            id,
            license_type: slug
        }
    }\
";

// OptionType {
//     id: "0bd82f7a-8aa1-4aa7-80e9-43e32a37f829",
//     slug: "assurance"
// }
// OptionType {
//     id: "7912cb1c-b5e1-4e21-8195-1ec2573fb609",
//     slug: "option_assurance"
// }
const GRAPHQL_GET_OPTIONS: &str = "\
    query getOptions {
        levels: option(
            where: {
                option_type_id: { _eq: \"0bd82f7a-8aa1-4aa7-80e9-43e32a37f829\" }
            }
        ) {
            id
            level: slug
        }
        options: option(
            where: {
                option_type_id: { _eq: \"7912cb1c-b5e1-4e21-8195-1ec2573fb609\" }
            }
        ) {
            id
            option: slug
        }
    }\
";

const GRAPHQL_GET_PRICES: &str = "\
    query getPrices(
        $products: [uuid!]!
        $levels: [uuid!]!
        $options: [uuid!]!
        $department_structure_id: Int!
        $region_structure_id: Int!
        $national_structure_id: Int!
        $season: Int!
    ) {
        products: price(
            where: {
                season_id: { _eq: $season }
                product_id: { _in: $products }
                structure_id: { _in: [ $department_structure_id, $region_structure_id, $national_structure_id ] }
                option_id: { _is_null: true }
            }
        ) {
            product_id
            structure_id
            price_in_cents: value
        }
        levels: price(
            where: {
                season_id: { _eq: $season }
                option_id: { _in: $levels }
                structure_id: { _eq: $national_structure_id }
                product_id: { _is_null: true }
            }
        ) {
            option_id
            price_in_cents: value
        }
        options: price(
            where: {
                season_id: { _eq: $season }
                option_id: { _in: $options }
                structure_id: { _eq: $national_structure_id }
                product_id: { _is_null: true }
            }
        ) {
            option_id
            price_in_cents: value
        }
    }\
";

const GRAPHQL_UPDATE_EMAIL: &str = "\
    mutation updateEmail(
        $user_id: uuid!
        $email: String!
        $alt_email: String!
    ) {
        result: update_UTI_Utilisateurs_by_pk(
            pk_columns: { id: $user_id }
            _set: { CT_Email: $email, CT_Email2: $alt_email }
        ) {
            id
        }
    }\
";

const GRAPHQL_UPDATE_ADDRESS_CITY: &str = "\
    mutation updateAddress(
        $id: uuid!
        $city: String!
        $zip: String!
        $insee: String!
    ) {
        result: update_ADR_Adresse(
            where: { ID_Utilisateur: { _eq: $id } }
            _set: {
                Ville: $city
                CodeInsee: $insee
                CodePostal: $zip
                # ID_Pays: 75
            }
        ) {
            affected_rows
        }
    }\
";

const GRAPHQL_INSERT_ADDRESS_CITY: &str = "\
    mutation insertAddress(
        $id: uuid!
        $city: String!
        $zip: String!
        $insee: String!
        $line1: String!
        $country_id: Int!
    ) {
        result: insert_ADR_Adresse_one(
            object: {
                ID_Utilisateur: $id
                Ville: $city
                CodeInsee: $insee
                CodePostal: $zip
                Adresse1: $line1
                ID_Pays: $country_id
            }
        ) {
            id
        }
    }\
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::season::current_season;
    use std::time::SystemTime;

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
    async fn test_member_by_license_number() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let result = member_by_license_number(154316).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(19750826, result.dob);
        assert_eq!("GRAS", result.last_name);
    }

    #[tokio::test]
    async fn test_licensee_by_last_name_and_dob() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = member_by_name_and_dob("Jerome", "DAVID", 19770522)
            .await
            .unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        println!("{}", results.len());
        let result = results.first().unwrap();
        assert_eq!(19770522, result.dob);
        assert_eq!("DAVID", result.last_name);
    }

    #[tokio::test]
    async fn test_member_by_ids() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = members_by_ids(
            [
                "6692903b-8032-43ea-8cd9-530f14bf5324",
                "5f5e0d27-cf50-42ea-89f8-f1649a2ef6aa",
            ]
            .as_slice(),
            None,
        )
        .await
        .unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(2, results.len());
        let result = results
            .iter()
            .find(|&it| it.metadata.license_number == Some(33109))
            .unwrap();
        assert_eq!(19770522, result.dob);
        assert_eq!("DAVID", result.last_name);
    }

    #[tokio::test]
    async fn test_structure_hierarchy() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let hierarchy = structure_hierarchy_by_id(*STRUCTURE_ID).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!(
            "{}, {}, {}, {}",
            hierarchy.id,
            hierarchy.department_structure_id,
            hierarchy.region_structure_id,
            hierarchy.national_structure_id
        );
    }

    #[tokio::test]
    async fn test_products() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let products = products().await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        for (license_type, license_name) in [
            (LicenseType::Adult, "Adult"),
            (LicenseType::Child, "Child"),
            (LicenseType::Family, "Family"),
            (LicenseType::NonMemberAdult, "Non Member Adult"),
            (LicenseType::NonMemberChild, "Non Member Child"),
        ] {
            assert!(
                products
                    .iter()
                    .find(|it| it.license_type.as_ref() == Some(&license_type))
                    .is_some(),
                "{}",
                license_name
            );
        }
    }

    #[tokio::test]
    async fn test_options() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let (insurance_levels, insurance_options) = options().await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        for (insurance_level, level_name) in [
            (InsuranceLevel::RC, "RC"),
            (InsuranceLevel::Base, "Base"),
            (InsuranceLevel::BasePlus, "Base+"),
            (InsuranceLevel::BasePlusPlus, "Base++"),
        ] {
            assert!(
                insurance_levels
                    .iter()
                    .find(|it| it.level.as_ref() == Some(&insurance_level))
                    .is_some(),
                "{}",
                level_name
            );
        }
        for (insurance_option, option_name) in [
            (InsuranceOption::MountainBike, "Mountain Bike"),
            (InsuranceOption::Ski, "Ski"),
            (InsuranceOption::SlacklineAndHighline, "Slackline/Highline"),
            (InsuranceOption::TrailRunning, "Trail Running"),
        ] {
            assert!(
                insurance_options
                    .iter()
                    .find(|it| it.option.as_ref() == Some(&insurance_option))
                    .is_some(),
                "{}",
                option_name
            );
        }
    }

    #[tokio::test]
    async fn test_prices() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let (license_prices, insurance_level_prices, insurance_option_prices) =
            prices(None).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        for (license_type, license_name) in [
            (LicenseType::Adult, "Adult"),
            (LicenseType::Child, "Child"),
            (LicenseType::Family, "Family"),
            (LicenseType::NonMemberAdult, "Non Member Adult"),
            (LicenseType::NonMemberChild, "Non Member Child"),
        ] {
            assert!(
                license_prices.get(&license_type).is_some(),
                "{}",
                license_name
            );
        }
        for (insurance_level, level_name) in [
            (InsuranceLevel::RC, "RC"),
            (InsuranceLevel::Base, "Base"),
            (InsuranceLevel::BasePlus, "Base+"),
            (InsuranceLevel::BasePlusPlus, "Base++"),
        ] {
            assert!(
                insurance_level_prices.get(&insurance_level).is_some(),
                "{}",
                level_name
            );
        }
        for (insurance_option, option_name) in [
            (InsuranceOption::MountainBike, "Mountain Bike"),
            (InsuranceOption::Ski, "Ski"),
            (InsuranceOption::SlacklineAndHighline, "Slackline/Highline"),
            (InsuranceOption::TrailRunning, "Trail Running"),
        ] {
            assert!(
                insurance_option_prices.get(&insurance_option).is_some(),
                "{}",
                option_name
            );
        }
    }

    #[tokio::test]
    async fn test_update_email() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let result = update_email(
            "6692903b-8032-43ea-8cd9-530f14bf5324",
            "programingjd@gmail.com",
            None,
        )
        .await;
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(result.is_some());
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

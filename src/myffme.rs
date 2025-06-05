use crate::address::City;
use crate::chrome::{ChromeVersion, CHROME_VERSION};
use crate::user::LicenseType::NonPracticing;
use crate::user::{Gender, LicenseType, MedicalCertificateStatus, Metadata, Structure};
use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER};
use hyper::http::{HeaderName, HeaderValue};
use pinboard::Pinboard;
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::{Client, Response, Url};
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::str::from_utf8;
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, SystemTime};
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::headers::JSON;
use tiered_server::norm::{normalize_first_name, normalize_last_name};
#[allow(unused_imports)]
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use tracing::debug;

pub async fn ffme_auth_update_loop() {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    update_chrome_version(timestamp).await;
    let _ = update_bearer_token(timestamp).await;
    thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async {
                loop {
                    let timestamp = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u32;
                    let chrome_version_timestamp =
                        CHROME_VERSION.get_ref().map(|it| it.timestamp).unwrap_or(0);
                    let mut success = true;
                    if timestamp > chrome_version_timestamp + USERAGENT_VALIDITY_SECONDS
                        && !update_chrome_version(timestamp).await
                    {
                        success = false;
                    }
                    let token_timestamp = MYFFME_AUTHORIZATION
                        .get_ref()
                        .map(|it| it.timestamp)
                        .unwrap_or(0);
                    if timestamp > token_timestamp + AUTHORIZATION_VALIDITY_SECONDS
                        && update_bearer_token(timestamp).await.is_none()
                    {
                        success = false;
                    }
                    sleep(Duration::from_secs(if success {
                        (15_000 + fastrand::i16(-1500..1500)) as u64
                    } else {
                        (600 + fastrand::i16(-100..100)) as u64
                    }))
                    .await;
                }
            })
    });
}

struct Authorization {
    bearer_token: HeaderValue,
    timestamp: u32,
}

#[derive(Deserialize)]
struct Token {
    token: String,
}

const AUTHORIZATION_VALIDITY_SECONDS: u32 = 36_000; // 10h
const USERAGENT_VALIDITY_SECONDS: u32 = 250_000; // ~3days

const USERNAME_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "MYFFME_USERNAME",
};
const PASSWORD_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "MYFFME_PASSWORD",
};

static USERNAME: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(USERNAME_KEY).expect("myffme username not set"));
//noinspection SpellCheckingInspection
static PASSWORD: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(PASSWORD_KEY).expect("myffme password not set"));

static MYFFME_AUTHORIZATION: LazyLock<Pinboard<Authorization>> = LazyLock::new(Pinboard::new_empty);

fn client() -> Client {
    let chrome_version = CHROME_VERSION
        .get_ref()
        .map(|it| it.chrome_version)
        .unwrap_or(135);
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, JSON);
    headers.insert(CONTENT_TYPE, JSON);
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::try_from(format!("\"Google Chrome\";v=\"{chrome_version}\", \"Not-A.Brand\";v=\"8\", \"Chromium\";v=\"{chrome_version}\"")).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"Windows\""),
    );
    let user_agent = format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome_version}.0.0.0 Safari/537.36"
    );
    Client::builder()
        .https_only(true)
        .use_rustls_tls()
        .min_tls_version(Version::TLS_1_3)
        .user_agent(HeaderValue::try_from(user_agent).unwrap())
        .http2_prior_knowledge()
        .redirect(Policy::none())
        .default_headers(headers)
        .deflate(true)
        .gzip(true)
        .brotli(true)
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(15))
        .build()
        .unwrap()
}

async fn update_chrome_version(timestamp: u32) -> bool {
    match client()
        .get("https://raw.githubusercontent.com/chromium/chromium/main/chrome/VERSION")
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(text) => {
                match text.lines().next().and_then(|it| {
                    let mut split = it.split('=');
                    let _ = split.next();
                    split.next().and_then(|it| it.parse::<u16>().ok())
                }) {
                    Some(chrome_version) => {
                        CHROME_VERSION.set(ChromeVersion {
                            chrome_version: chrome_version - 2,
                            timestamp,
                        });
                        true
                    }
                    None => {
                        debug!("failed to parse chrome version");
                        false
                    }
                }
            }
            Err(err) => {
                debug!("failed to get chrome verson file from github:\n{err:?}");
                false
            }
        },
        Err(err) => {
            debug!("failed to get response from github for the chrome verson file:\n{err:?}");
            false
        }
    }
}

pub async fn update_bearer_token(timestamp: u32) -> Option<String> {
    match client()
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
                #[cfg(test)]
                println!("bearer token: {}", bearer_token.to_str().unwrap());
                MYFFME_AUTHORIZATION.set(Authorization {
                    bearer_token,
                    timestamp,
                });
                Some(token.token)
            }
            Err(err) => {
                debug!("failed to parse login response:\n{err:?}");
                None
            }
        },
        Err(err) => {
            debug!("failed to get login response:\n{err:?}");
            None
        }
    }
}

const APPROXIMATE_NUMBER_OF_SECS_IN_YEAR: u32 = 31_557_600;

pub fn current_season(timestamp: Option<u32>) -> u16 {
    let year_2020_utc_start_timestamp = 1577836800_u32;
    let elapsed = timestamp.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32
    }) - year_2020_utc_start_timestamp;
    // can be off by 1 but won't change the result
    let years = elapsed / APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let current_year_elapsed_seconds = elapsed - years * APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let years = years as u16;
    let seconds_between_jan_and_august = if years % 4 == 0 {
        18_316_800
    } else {
        18_230_400
    };
    if current_year_elapsed_seconds > seconds_between_jan_and_august {
        2020 + years + 1
    } else {
        2020 + years
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
    let mut results = users_response_to_members(response).await?;
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
    let mut iter = users_response_to_members(response).await?.into_iter();
    let first = iter.next()?;
    if iter.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub async fn members_by_structure(structure_id: u32) -> Option<Vec<Member>> {
    let response = users_response_by_structure(structure_id).await?;
    users_response_to_members(response).await
}

async fn users_response_by_license_numbers(license_numbers: &[u32]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    client.execute(request).await.ok()
}

async fn users_response_by_ids(ids: &[&str]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
            "operationName": "getUsersByUserIds",
            "query": GRAPHQL_GET_USERS_BY_IDS,
            "variables": {
                "ids": ids
            }
        }))
        .build()
        .ok()?;
    client.execute(request).await.ok()
}

async fn users_response_by_structure(structure_id: u32) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    client.execute(request).await.ok()
}

async fn users_response_by_dob(dob: u32) -> Option<Response> {
    let s = dob.to_string();
    let dob = s.as_bytes();
    let yyyy = from_utf8(&dob[..4]).unwrap();
    let mm = from_utf8(&dob[4..6]).unwrap();
    let dd = from_utf8(&dob[6..]).unwrap();
    let dob = format!("{yyyy}-{mm}-{dd}");
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    client.execute(request).await.ok()
}

async fn users_response_to_members(response: Response) -> Option<Vec<Member>> {
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
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let users = response.json::<GraphqlResponse>().await.ok()?.data.list;
    let user_ids = users.iter().map(|it| it.id.as_str()).collect::<Vec<_>>();
    let mut addresses = user_addresses(&user_ids).await?;
    let mut licenses = user_licenses(&user_ids).await?;
    let mut medical_certificates = user_medical_certificates(&user_ids).await?;
    let mut health_questionnaires = user_health_questionnaires(&user_ids).await?;
    let structure_ids = licenses
        .values()
        .map(|it| it.structure_id)
        .collect::<Vec<_>>();
    let structures = structures_by_ids(&structure_ids).await?;
    Some(
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
                    },
                }
            })
            .collect(),
    )
}

async fn user_medical_certificates(ids: &[&str]) -> Option<BTreeMap<String, Document>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    let documents = response.json::<GraphqlResponse>().await.ok()?.data.list;
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

async fn user_health_questionnaires(ids: &[&str]) -> Option<BTreeMap<String, Document>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    let documents = response.json::<GraphqlResponse>().await.ok()?.data.list;
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

async fn user_addresses(ids: &[&str]) -> Option<BTreeMap<String, Address>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    let addresses = response.json::<GraphqlResponse>().await.ok()?.data.list;
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

async fn user_licenses(ids: &[&str]) -> Option<BTreeMap<String, License>> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
    let licenses = response.json::<GraphqlResponse>().await.ok()?.data.list;
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
    let client = client();
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
    let structures = response.json::<GraphqlResponse>().await.ok()?.data.list;
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

pub async fn update_address(
    user_id: &str,
    zip_code: &str,
    city: &City,
    line1: Option<String>,
    country_id: Option<u16>,
) -> Option<()> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = client();
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
                .ok()?
                .data
                .result
                .affected_rows
        };
        #[cfg(not(test))]
        let affected_rows = response
            .json::<GraphqlResponse>()
            .await
            .ok()?
            .data
            .result
            .affected_rows;
        if affected_rows > 0 {
            Some(())
        } else {
            let client = crate::myffme::client();
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

#[derive(Deserialize, Serialize)]
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
        Ok(str) => Ok(Some(str.try_into().map_err(Error::custom)?)),
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
    ) {
        list: licence(
            where: { user_id: { _in: $ids } }
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

const GRAPHQL_GET_MEDICAL_CERTIFICATES_BY_USER_IDS: &str = "\
    query getMedicalCertificatesByUserIds(
        $ids: [uuid!]!
    ) {
        list: DOC_Document(
            distinct_on: ID_Utilisateur
            order_by: [ { ID_Utilisateur: asc }, { ID_Saison: desc_nulls_last } ]
            where: {
                ID_Utilisateur: { _in: $ids }
                EST_DocumentValide: { _eq: true }
                EST_Actif: { _eq: true }
                ID_Type_Document: { _in: [ 5, 6, 7, 9 ] }
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
    ) {
        list: DOC_Document(
            distinct_on: ID_Utilisateur
            order_by: [ { ID_Utilisateur: asc }, { ID_Saison: desc_nulls_last } ]
            where: {
                ID_Utilisateur: { _in: $ids }
                EST_DocumentValide: { _eq: true }
                EST_Actif: { _eq: true }
                ID_Type_Document: { _in: [ 60 ] }
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
    use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeZone, Utc};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_current_season() {
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 03, 12).unwrap(),
        ));
        let season = current_season(Some(date.timestamp() as u32));
        assert_eq!(2021, season);
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 09, 01).unwrap(),
        ));
        let season = current_season(Some(date.timestamp() as u32));
        assert_eq!(2022, season);
        let date = Utc
            .timestamp_millis_opt(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
            )
            .unwrap();
        let season = current_season(None);
        assert_eq!(season, current_season(Some(date.timestamp() as u32)));
        let mut year = date.year() as u16;
        let month = date.month() as u16;
        let day = date.day() as u16;
        if month == 7 && day > 29 {
            return;
        }
        if month == 8 && day < 3 {
            return;
        }
        if month == 8 {
            year += 1;
        }
        assert_eq!(year, season);
    }

    #[tokio::test]
    async fn test_member_by_license_number() {
        assert!(update_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let result = member_by_license_number(154316).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(19750826, result.dob);
        assert_eq!("GRAS", result.last_name);
    }

    #[tokio::test]
    async fn test_licensee_by_last_name_and_dob() {
        assert!(update_bearer_token(0).await.is_some());
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
        println!("{}", serde_json::to_string(result).unwrap())
    }

    #[tokio::test]
    async fn test_list() {
        assert!(update_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = members_by_structure(10).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        println!("{}", results.len());
        println!("{}", serde_json::to_string(&results).unwrap());
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(".list.json")
            .await
            .ok()
            .unwrap()
            .write_all(serde_json::to_string(&results).unwrap().as_bytes())
            .await
            .unwrap();
    }
}

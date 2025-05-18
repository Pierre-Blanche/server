use crate::address::City;
use crate::chrome::{ChromeVersion, CHROME_VERSION};
use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER};
use hyper::http::{HeaderName, HeaderValue};
use pinboard::Pinboard;
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::{Client, Url};
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, SystemTime};
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::headers::JSON;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use tracing::{debug, warn};

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
                    if timestamp > chrome_version_timestamp + USERAGENT_VALIDITY_SECONDS {
                        if !update_chrome_version(timestamp).await {
                            success = false;
                        }
                    }
                    let token_timestamp = MYFFME_AUTHORIZATION
                        .get_ref()
                        .map(|it| it.timestamp)
                        .unwrap_or(0);
                    if timestamp > token_timestamp + AUTHORIZATION_VALIDITY_SECONDS {
                        if update_bearer_token(timestamp).await.is_none() {
                            success = false;
                        }
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

static MYFFME_AUTHORIZATION: LazyLock<Pinboard<Authorization>> =
    LazyLock::new(|| Pinboard::new_empty());

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

fn deserialize_date<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    let date = s
        .split('T')
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let mut split = date.split('-');
    let yyyy = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let mm = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let dd = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    format!("{yyyy}{mm}{dd}")
        .parse()
        .map_err(serde::de::Error::custom)
}

fn deserialize_license_number<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

const APPROXIMATE_NUMBER_OF_SECS_IN_YEAR: u32 = 31_557_600;

fn current_season(timestamp: Option<u32>) -> u16 {
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
pub struct LicenseeInfo {
    pub licensee: Licensee,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_license_season: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_structure_name: Option<String>,
}

pub async fn search(
    name: Option<&str>,
    dob: Option<u32>,
    license_number: Option<u32>,
) -> Option<Vec<LicenseeInfo>> {
    #[derive(Deserialize)]
    pub(crate) struct SearchResult {
        #[serde(rename = "fullname")]
        pub(crate) name: String,
        #[serde(rename = "birthdate", deserialize_with = "deserialize_date")]
        pub(crate) dob: u32,
        #[serde(
            rename = "licenceNumber",
            deserialize_with = "deserialize_license_number"
        )]
        pub(crate) license_number: u32,
        #[serde(rename = "season")]
        pub(crate) latest_license_season: Option<u16>,
        #[serde(rename = "structure")]
        pub(crate) latest_structure_name: Option<String>,
    }
    let mut url = Url::parse("https://api.core.myffme.fr/api/users/licensee/search").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("page", "1");
    query.append_pair("itemsPerPage", "1000");
    if let Some(name) = name {
        query.append_pair("input", name);
    }
    if let Some(dob) = dob {
        let s = dob.to_string();
        query.append_pair(
            "birthdate",
            &format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8]),
        );
    }
    if let Some(license_number) = license_number {
        query.append_pair("licenceNumber", &format!("{}", license_number));
    }
    drop(query);
    debug!("GET {}", url.as_str());
    let client = client();
    let request = client
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    let search_results = {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let mut file_name = ".search".to_string();
        if let Some(name) = name {
            file_name.push('_');
            file_name.push_str(name);
        }
        if let Some(dob) = dob {
            file_name.push('_');
            file_name.push_str(dob.to_string().as_str());
        }
        file_name.push_str(".json");
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
        serde_json::from_str::<Vec<SearchResult>>(&text).ok()?
    };
    #[cfg(not(test))]
    let search_results = response.json::<Vec<SearchResult>>().await.ok()?;
    let mut infos = search_results
        .into_iter()
        .map(|it| {
            (
                it.license_number,
                (it.latest_license_season, it.latest_structure_name),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let body = json!({
        "operationName": "getUsersByLicenseNumbers",
        "query": GRAPHQL_GET_USERS_BY_LICENSE_NUMBER,
        "variables": {
            "license_numbers": &infos.keys().collect::<Vec<_>>(),
        }
    });
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
        .json(&body)
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct UserList {
        #[serde(rename = "UTI_Utilisateurs")]
        list: Vec<Licensee>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: UserList,
    }
    #[cfg(test)]
    let licensees = {
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let mut file_name = ".users".to_string();
        if let Some(name) = name {
            file_name.push('_');
            file_name.push_str(name);
        }
        if let Some(dob) = dob {
            file_name.push('_');
            file_name.push_str(dob.to_string().as_str());
        }
        file_name.push_str(".json");
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
            .list
    };
    #[cfg(not(test))]
    let licensees = response.json::<GraphqlResponse>().await.ok()?.data.list;

    let ids = licensees
        .iter()
        .map(|it| it.id.as_str())
        .collect::<Vec<_>>();

    let mut addresses = user_addresses(&ids).await?;

    Some(
        licensees
            .into_iter()
            .map(|mut licensee| {
                licensee.address = addresses.remove(licensee.id.as_str());
                let info = infos.remove(&licensee.license_number);
                LicenseeInfo {
                    licensee,
                    latest_license_season: match info {
                        Some((Some(latest_season), _)) => Some(latest_season),
                        _ => None,
                    },
                    latest_structure_name: match info {
                        Some((_, Some(structure))) => Some(structure),
                        _ => None,
                    },
                }
            })
            .collect(),
    )
}

pub async fn current_licensees() -> Option<Vec<Licensee>> {
    let season = current_season(None);
    licensees_from_ids(licensees_metadata(season).await?, season).await
}

pub async fn user_addresses(ids: &[&str]) -> Option<BTreeMap<String, Address>> {
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
        #[serde(rename = "ADR_Adresse")]
        list: Vec<Address>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: AddressList,
    }
    #[cfg(test)]
    let addresses = {
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

#[derive(Default)]
struct UserMetadata {
    license_type: Option<LicenseType>,
    medical_certificate_status: Option<MedicalCertificateStatus>,
}

async fn licensees_metadata(season: u16) -> Option<BTreeMap<String, UserMetadata>> {
    #[derive(Deserialize)]
    struct User {
        id: String,
    }
    #[derive(Deserialize)]
    struct Product {
        slug: String,
    }
    #[derive(Deserialize)]
    struct License {
        user: User,
        product: Product,
        status: String,
    }
    let mut url = Url::parse("https://api.core.myffme.fr/api/licences").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("page", "1");
    query.append_pair("itemsPerPage", "1000");
    query.append_pair("structure", "10");
    query.append_pair("season", season.to_string().as_str());
    drop(query);
    debug!("GET {}", url.as_str());
    let client = client();
    let request = client
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    let licenses = {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".users_metadata_{season}.json");
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
        serde_json::from_str::<Vec<License>>(&text).ok()?
    };
    #[cfg(not(test))]
    let licenses = response.json::<Vec<License>>().await.ok()?;
    Some(
        licenses
            .into_iter()
            .map(|it| {
                (
                    it.user.id,
                    UserMetadata {
                        medical_certificate_status: it.status.as_str().try_into().ok(),
                        license_type: it.product.slug.as_str().try_into().ok(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

async fn licensees_from_ids(
    mut metadata: BTreeMap<String, UserMetadata>,
    season: u16,
) -> Option<Vec<Licensee>> {
    let ids = metadata.keys().map(|it| it.as_str()).collect::<Vec<_>>();
    let mut addresses = user_addresses(&ids).await?;
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
            "operationName": "getUsersByIds",
            "query": GRAPHQL_GET_USERS_BY_IDS,
            "variables": {
                "ids": &ids,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct UserList {
        #[serde(rename = "UTI_Utilisateurs")]
        list: Vec<Licensee>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: UserList,
    }
    #[cfg(test)]
    let licensees = {
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".users_{season}.json");
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
            .list
    };
    #[cfg(not(test))]
    let licensees = response.json::<GraphqlResponse>().await.ok()?.data.list;
    Some(
        licensees
            .into_iter()
            .map(|mut it| {
                it.address = addresses.remove(&it.id);
                if let Some(meta) = metadata.remove(&it.id) {
                    it.license_type = meta.license_type;
                    it.medical_certificate_status = meta.medical_certificate_status;
                }
                it.last_license_season = Some(season);
                it
            })
            .collect(),
    )
}

pub async fn update_address(user_id: &str, zip_code: &str, city: &City) -> Option<()> {
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
    let success = (&response.status()).is_success();
    #[cfg(test)]
    {
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
    }
    if success { Some(()) } else { None }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Female,
    Male,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    Adult,
    Child,
    Family,
    NonMemberAdult,
    NonMemberChild,
}

impl TryFrom<&str> for LicenseType {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "licence_adulte" => Ok(LicenseType::Adult),
            "hors_club_adulte" => Ok(LicenseType::NonMemberAdult),
            "licence_jeune" => Ok(LicenseType::Child),
            "hors_club_jeune" => Ok(LicenseType::NonMemberChild),
            "licence_famille" => Ok(LicenseType::Family),
            _ => Err("unknown license type"),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MedicalCertificateStatus {
    Recreational,
    Competition,
    HealthQuestionnaire,
    WaitingForDocument,
}

impl TryFrom<&str> for MedicalCertificateStatus {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "loisir" => Ok(MedicalCertificateStatus::Recreational),
            "competition" => Ok(MedicalCertificateStatus::Competition),
            "waiting_document" => Ok(MedicalCertificateStatus::WaitingForDocument),
            "qs" => Ok(MedicalCertificateStatus::HealthQuestionnaire),
            _ => Err("unknown medical certificate status"),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Licensee {
    pub id: String,
    #[serde(deserialize_with = "deserialize_gender")]
    pub gender: Gender,
    pub first_name: String,
    pub last_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_name: Option<String>,
    #[serde(deserialize_with = "deserialize_date")]
    pub dob: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_phone_number: Option<String>,
    pub license_number: u32,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_place: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_place_insee: Option<String>,
    pub active_license: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_address",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) address: Option<Address>,
    #[serde(
        default,
        deserialize_with = "deserialize_license_type",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) license_type: Option<LicenseType>,
    #[serde(
        default,
        deserialize_with = "deserialize_medical_certificate_status",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) medical_certificate_status: Option<MedicalCertificateStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_license_season: Option<u16>,
}

#[derive(Deserialize, Serialize)]
pub struct Address {
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

fn deserialize_address<'de, D>(deserializer: D) -> Result<Option<Address>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <Vec<Address>>::deserialize(deserializer);
    match result {
        Ok(it) => Ok(it.into_iter().next()),
        Err(_err) => Ok(None),
    }
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
        Ok(str) => Ok(Some(str.try_into().map_err(|msg| Error::custom(msg))?)),
        Err(_err) => Ok(None),
    }
}

fn deserialize_medical_certificate_status<'de, D>(
    deserializer: D,
) -> Result<Option<MedicalCertificateStatus>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(Some(str.try_into().map_err(|msg| Error::custom(msg))?)),
        Err(_err) => Ok(None),
    }
}

const X_HASURA_ROLE: HeaderName = HeaderName::from_static("x-hasura-role");
const ADMIN: HeaderValue = HeaderValue::from_static("admin");

const GRAPHQL_GET_USERS_BY_IDS: &str = "\
    query getUsersByIds($ids: [uuid!]!) {
        UTI_Utilisateurs(where: {id: {_in: $ids}}) {
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
            active_license: EST_Licencie
            __typename
        }
    }\
";

const GRAPHQL_GET_ADDRESSES_BY_USER_IDS: &str = "\
    query getAddressesByUserIds($ids: [uuid!]!) {
        ADR_Adresse(
            where: {ID_Utilisateur: {_in: $ids}},
            order_by: [{ ID_Utilisateur: asc }, { Z_DateModification: desc }],
            distinct_on: [ID_Utilisateur]
        ) {
            user_id: ID_Utilisateur
            line1: Adresse1
            line2: Adresse2
            insee: CodeInsee
            zip_code: CodePostal,
            city: Ville
            __typename
        }
    }\
";

const GRAPHQL_GET_USERS_BY_LICENSE_NUMBER: &str = "\
    query getUsersByLicenseNumbers($license_numbers: [bigint!]!) {
        UTI_Utilisateurs(where: {LicenceNumero: {_in: $license_numbers}}) {
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
            active_license: EST_Licencie
            __typename
        }
    }\
";

const GRAPHQL_UPDATE_ADDRESS_CITY: &str = "\
    mutation updateAddress($id: uuid!, $city: String!, $zip: String!, $insee: String!) {
        update_ADR_Adresse(
            where: { ID_Utilisateur: {_eq: $id}},
            _set: {
                Ville: $city,
                CodeInsee: $insee,
                CodePostal: $zip
            }
        ) {
            affected_rows
            returning {
                id
                ID_Utilisateur
                Ville
                CodeInsee
                CodePostal
            }
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
    async fn test_search_by_license_number() {
        assert!(update_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = search(None, None, Some(154316)).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        println!("{}", results.len());
        let result = results.first().unwrap();
        assert_eq!(19750826, result.licensee.dob);
        assert_eq!("GRAS", result.licensee.last_name);
        println!("{}", serde_json::to_string(result).unwrap())
    }

    #[tokio::test]
    async fn test_search_by_name_and_dob() {
        assert!(update_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = search(Some("DAVID"), Some(19770522), None).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        println!("{}", results.len());
        let result = results.first().unwrap();
        assert_eq!(19770522, result.licensee.dob);
        assert_eq!("DAVID", result.licensee.last_name);
        println!("{}", serde_json::to_string(result).unwrap())
    }

    #[tokio::test]
    async fn test_list() {
        assert!(update_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = current_licensees().await.unwrap();
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

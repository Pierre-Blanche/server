use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER};
use hyper::http::{HeaderName, HeaderValue};
use pinboard::Pinboard;
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::{Client, Url};
use serde::de::Error;
use serde::Deserialize;
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

pub(crate) async fn ffme_auth_update_loop() {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    update_chrome_version(timestamp).await;
    update_bearer_token(timestamp).await;
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
                        if !update_bearer_token(timestamp).await {
                            success = false;
                        }
                    }
                    sleep(Duration::from_secs(if success {
                        15_000 + fastrand::i16(-1500..1500) as u64
                    } else {
                        600 + fastrand::i16(-100..100) as u64
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

struct ChromeVersion {
    chrome_version: u16,
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
static CHROME_VERSION: LazyLock<Pinboard<ChromeVersion>> = LazyLock::new(|| Pinboard::new_empty());

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
        .timeout(std::time::Duration::from_secs(5))
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

async fn update_bearer_token(timestamp: u32) -> bool {
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
                MYFFME_AUTHORIZATION.set(Authorization {
                    bearer_token: HeaderValue::try_from(format!("Bearer {}", token.token)).unwrap(),
                    timestamp,
                });
                true
            }
            Err(err) => {
                debug!("failed to parse login response:\n{err:?}");
                false
            }
        },
        Err(err) => {
            debug!("failed to get login response:\n{err:?}");
            false
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

fn current_season() -> u16 {
    let year_2020_utc_start_timestamp = 1577836800_u32;
    let elapsed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32
        - year_2020_utc_start_timestamp;
    // can be off by 1 but won't change the result
    let years = (elapsed as f32 / 365.25_f32).round() as u16;
    let current_year_elapsed_seconds = elapsed - (years as f32 * 365.25_f32).round() as u32;
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

pub(crate) struct LicenseeInfo {
    pub(crate) licensee: Licensee,
    pub(crate) latest_license_season: Option<u16>,
    pub(crate) latest_structure_name: Option<String>,
}

pub(crate) async fn search(name: Option<&str>, dob: Option<u32>) -> Option<Vec<LicenseeInfo>> {
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
            .map(|licensee| {
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

pub(crate) async fn list_current_licensees() -> Option<Vec<Licensee>> {
    list_from_ids(list_user_metadata().await?).await
}

struct UserMetadata {
    license_type: Option<LicenseType>,
    medical_certificate_status: Option<MedicalCertificateStatus>,
}

async fn list_user_metadata() -> Option<BTreeMap<String, UserMetadata>> {
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
    let mut url = Url::parse("https://app.myffme.fr/api/users/licensee/search").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("page", "1");
    query.append_pair("itemsPerPage", "1000");
    query.append_pair("structure", "10");
    query.append_pair("season", current_season().to_string().as_str());
    drop(query);
    debug!("GET {}", url.as_str());
    client()
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .send()
        .await
        .ok()?
        .json::<Vec<License>>()
        .await
        .map(|it| {
            it.into_iter()
                .map(|it| {
                    (
                        it.user.id,
                        UserMetadata {
                            medical_certificate_status: it.status.as_str().try_into().ok(),
                            license_type: it.product.slug.as_str().try_into().ok(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .ok()
}

async fn list_from_ids(mut metadata: BTreeMap<String, UserMetadata>) -> Option<Vec<Licensee>> {
    let current_season = current_season();
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let licensees = client()
        .get(url.as_str())
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
                "ids": metadata.keys().collect::<Vec<_>>(),
            }
        }))
        .send()
        .await
        .ok()?
        .json::<Vec<Licensee>>()
        .await
        .ok()?;
    Some(
        licensees
            .into_iter()
            .map(|mut it| {
                if let Some(meta) = metadata.remove(&it.id) {
                    it.license_type = meta.license_type;
                    it.medical_certificate_status = meta.medical_certificate_status;
                }
                it.last_license_season = Some(current_season);
                it
            })
            .collect(),
    )
}

#[derive(Deserialize)]
pub enum Gender {
    Female,
    Male,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
pub enum MedicalCertificateStatus {
    Recreational,
    Competition,
    HealthQuestionnaire,
    WaitingDocument,
}

impl TryFrom<&str> for MedicalCertificateStatus {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "loisir" => Ok(MedicalCertificateStatus::Recreational),
            "competition" => Ok(MedicalCertificateStatus::Competition),
            "waiting_document" => Ok(MedicalCertificateStatus::WaitingDocument),
            "qs" => Ok(MedicalCertificateStatus::HealthQuestionnaire),
            _ => Err("unknown medical certificate status"),
        }
    }
}

#[derive(Deserialize)]
pub struct Licensee {
    pub(crate) id: String,
    #[serde(deserialize_with = "deserialize_gender")]
    pub(crate) gender: Gender,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) birth_name: Option<String>,
    #[serde(deserialize_with = "deserialize_date")]
    pub(crate) dob: u32,
    pub(crate) email: Option<String>,
    pub(crate) alt_email: Option<String>,
    pub(crate) phone_number: String,
    pub(crate) alt_phone_number: Option<String>,
    pub(crate) license_number: u32,
    pub(crate) username: String,
    pub(crate) birth_place: String,
    pub(crate) birth_place_insee: Option<String>,
    pub(crate) active_license: bool,
    #[serde(default, deserialize_with = "deserialize_address")]
    pub(crate) address: Option<Address>,
    #[serde(default, deserialize_with = "deserialize_license_type")]
    pub(crate) license_type: Option<LicenseType>,
    #[serde(default, deserialize_with = "deserialize_medical_certificate_status")]
    pub(crate) medical_certificate_status: Option<MedicalCertificateStatus>,
    #[serde(default)]
    pub(crate) last_license_season: Option<u16>,
}

#[derive(Deserialize)]
pub struct Address {
    pub(crate) line1: Option<String>,
    pub(crate) line2: Option<String>,
    pub(crate) insee: Option<String>,
    pub(crate) zip_code: Option<String>,
    pub(crate) city: Option<String>,
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
            gender: CT_EST_Civilite,
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
            address: ADR_Adresse {
              line1: Adresse1
              line2: Adresse2
              insee: CodeInsee
              zip_code: CodePostal
              city: Ville
              __typename
            }
            __typename
        }
    }\
";

const GRAPHQL_GET_USERS_BY_LICENSE_NUMBER: &str = "\
    query getUsersByLicenseNumbers($license_numbers: [bigint!]!) {
        UTI_Utilisateurs(where: {LicenceNumero: {_in: $license_numbers}}) {
            id
            gender: CT_EST_Civilite,
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
            address: ADR_Adresse {
              line1: Adresse1
              line2: Adresse2
              insee: CodeInsee
              zip_code: CodePostal
              city: Ville
              __typename
            }
            __typename
        }
    }\
";

#[cfg(test)]
mod tests {
    use crate::myffme::{search, update_bearer_token};

    #[tokio::test]
    async fn save_list() {
        assert!(update_bearer_token(0).await);
        search(Some("DAVID"), Some(19770522)).await.unwrap();
    }
}

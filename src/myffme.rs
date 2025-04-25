use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER};
use hyper::http::{HeaderName, HeaderValue};
use pinboard::Pinboard;
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, SystemTime};
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::headers::JSON;
use tokio::time::sleep;
use tracing::{debug, warn};

pub(crate) fn ffme_auth_update_loop() {
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
        HeaderName::from_static("Sec-Ch-Ua"),
        HeaderValue::try_from(format!("\"Google Chrome\";v=\"{chrome_version}\", \"Not-A.Brand\";v=\"8\", \"Chromium\";v=\"{chrome_version}\"")).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("Sec-Ch-Ua-Mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("Sec-Ch-Ua-Platform"),
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

#[derive(Deserialize)]
pub(crate) struct Licensee {
    #[serde(rename = "fullname")]
    pub(crate) name: String,
    #[serde(rename = "birthdate", deserialize_with = "deserialize_date")]
    pub(crate) dob: u32,
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

pub(crate) async fn search(name: Option<&str>, dob: Option<u32>) -> Option<Vec<Licensee>> {
    let mut url = Url::parse("https://app.myffme.fr/api/users/licensee/search").unwrap();
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
    client()
        // headers.insert(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"));
        // headers.insert(
        //     REFERER,
        //     HeaderValue::from_static("https://app.myffme.fr/authentification"),
        // );
        // headers.insert(COOKIE, HeaderValue::from_static("myffme-session="));
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
        .json()
        .await
        .ok()
}

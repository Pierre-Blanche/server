use crate::chrome::CHROME_VERSION;
use hyper::header::ACCEPT_LANGUAGE;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::Client;
use std::convert::TryFrom;
use std::time::Duration;
use tiered_server::headers::JSON;

const SEC_CH_UA: HeaderName = HeaderName::from_static("sec-ch-ua");
const SEC_CH_UA_MOBILE: HeaderName = HeaderName::from_static("sec-ch-ua-mobile");
const SEC_CH_UA_MOBILE_VALUE_IS_DESKTOP: HeaderValue = HeaderValue::from_static("?0");
const SEC_CH_UA_PLATFORM: HeaderName = HeaderName::from_static("sec-ch-ua-platform");
const SEC_CH_UA_PLATFORM_VALUE_WINDOWS: HeaderValue = HeaderValue::from_static("\"Windows\"");
const DOCUMENT: HeaderValue = HeaderValue::from_static(
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
);
const FR: HeaderValue = HeaderValue::from_static("fr-FR,fr;q=0.9");
const SEC_FETCH_DEST: HeaderName = HeaderName::from_static("sec-fetch-dest");
const SEC_FETCH_MODE: HeaderName = HeaderName::from_static("sec-fetch-mode");
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const SEC_FETCH_USER: HeaderName = HeaderName::from_static("sec-fetch-user");
const SEC_FETCH_DEST_DOCUMENT: HeaderValue = HeaderValue::from_static("document");
const SEC_FETCH_MODE_NAVIGATE: HeaderValue = HeaderValue::from_static("navigate");
const SEC_FETCH_SITE_NONE: HeaderValue = HeaderValue::from_static("none");
const SEC_FETCH_USER_ANONYMOUS: HeaderValue = HeaderValue::from_static("?1");

pub(crate) fn json_client() -> Client {
    let chrome_version = CHROME_VERSION
        .get_ref()
        .map(|it| it.chrome_version)
        .unwrap_or(135);
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, JSON);
    headers.insert(CONTENT_TYPE, JSON);
    headers.insert(SEC_CH_UA,
        HeaderValue::try_from(format!("\"Google Chrome\";v=\"{chrome_version}\", \"Not-A.Brand\";v=\"8\", \"Chromium\";v=\"{chrome_version}\"")).unwrap(),
    );
    headers.insert(SEC_CH_UA_MOBILE, SEC_CH_UA_MOBILE_VALUE_IS_DESKTOP);
    headers.insert(SEC_CH_UA_PLATFORM, SEC_CH_UA_PLATFORM_VALUE_WINDOWS);
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
        .zstd(true)
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(15))
        .build()
        .unwrap()
}

pub(crate) fn html_client() -> Client {
    let chrome_version = CHROME_VERSION
        .get_ref()
        .map(|it| it.chrome_version)
        .unwrap_or(135);
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, DOCUMENT);
    headers.insert(ACCEPT_LANGUAGE, FR);
    headers.insert(SEC_CH_UA,
                   HeaderValue::try_from(format!("\"Google Chrome\";v=\"{chrome_version}\", \"Not-A.Brand\";v=\"8\", \"Chromium\";v=\"{chrome_version}\"")).unwrap(),
    );
    headers.insert(SEC_CH_UA_MOBILE, SEC_CH_UA_MOBILE_VALUE_IS_DESKTOP);
    headers.insert(SEC_CH_UA_PLATFORM, SEC_CH_UA_PLATFORM_VALUE_WINDOWS);
    headers.insert(SEC_FETCH_DEST, SEC_FETCH_DEST_DOCUMENT);
    headers.insert(SEC_FETCH_MODE, SEC_FETCH_MODE_NAVIGATE);
    headers.insert(SEC_FETCH_SITE, SEC_FETCH_SITE_NONE);
    headers.insert(SEC_FETCH_USER, SEC_FETCH_USER_ANONYMOUS);
    let user_agent = format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome_version}.0.0.0 Safari/537.36"
    );
    Client::builder()
        .https_only(true)
        .use_rustls_tls()
        .min_tls_version(Version::TLS_1_3)
        .user_agent(HeaderValue::try_from(user_agent).unwrap())
        .redirect(Policy::none())
        .default_headers(headers)
        .deflate(true)
        .gzip(true)
        .brotli(true)
        .zstd(true)
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(15))
        .build()
        .unwrap()
}

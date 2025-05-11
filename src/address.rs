use crate::chrome::CHROME_VERSION;
use crate::myffme::Address;
use hyper::header::{
    HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER,
};
use hyper::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::tls::Version;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tiered_server::headers::JSON;
use tokio::io::AsyncWriteExt;
use tracing::debug;

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
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

#[derive(Deserialize, Serialize)]
pub(crate) struct City {
    #[serde(rename = "nom")]
    name: String,
    #[serde(rename = "code")]
    insee: String,
}

pub(crate) async fn city_by_zip_code(zip_code: &str) -> Option<Vec<City>> {
    let mut url = Url::parse("https://geo.api.gouv.fr/communes").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("codePostal", zip_code);
    drop(query);
    debug!("GET {}", url.as_str());
    let client = client();
    let request = client.get(url.as_str()).build().ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".city_{zip_code}.json");
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
        serde_json::from_str::<Vec<City>>(&text).ok()
    }
    #[cfg(not(test))]
    response.json::<Vec<City>>().await.ok()
}

pub(crate) async fn address(insee: Option<&str>, text: &str) -> Option<Vec<Address>> {
    #[derive(Deserialize)]
    struct Addr {
        name: String,
        #[serde(rename = "postcode")]
        zip_code: String,
        #[serde(rename = "citycode")]
        insee: Option<String>,
        city: String,
    }
    #[derive(Deserialize)]
    struct Feature {
        properties: Addr,
    }
    #[derive(Deserialize)]
    struct FeatureCollection {
        features: Vec<Feature>,
    }
    let mut url = Url::parse("https://api-adresse.data.gouv.fr/search/").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("q", text);
    query.append_pair("type", "housenumber");
    query.append_pair("limit", "10");
    if let Some(insee) = insee {
        query.append_pair("citycode", insee);
    }
    drop(query);
    debug!("GET {}", url.as_str());
    let client = client();
    let request = client.get(url.as_str()).build().ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    let features = {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".address.json";
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
        serde_json::from_str::<FeatureCollection>(&text)
            .ok()?
            .features
    };
    #[cfg(not(test))]
    let features = response.json::<FeatureCollection>().await.ok()?.features;
    Some(
        features
            .into_iter()
            .map(
                |Feature {
                     properties:
                         Addr {
                             name,
                             zip_code,
                             insee,
                             city,
                             ..
                         },
                 }| {
                    Address {
                        user_id: None,
                        line1: Some(name),
                        line2: None,
                        insee,
                        zip_code: Some(zip_code),
                        city: Some(city),
                    }
                },
            )
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_search_by_zip_code() {
        let t0 = SystemTime::now();
        let results = city_by_zip_code("85200").await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(12, results.len());
        assert!(results.iter().any(|it| &it.insee == "85092"));
    }

    #[tokio::test]
    async fn test_search_address() {
        let t0 = SystemTime::now();
        let results = address(Some("85092"), "100 rue").await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|it| it.zip_code.as_deref() == Some("85200"))
        );
    }
}

use crate::http_client::json_client;
use crate::myffme::address::Address;
use reqwest::Url;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tokio::io::AsyncWriteExt;
use tracing::debug;

#[derive(Deserialize, Serialize)]
pub struct City {
    #[serde(rename = "nom")]
    pub name: String,
    #[serde(rename = "code")]
    pub insee: String,
}

pub async fn city_name_by_insee(insee: &str) -> Option<String> {
    let mut url = Url::parse(&format!("https://geo.api.gouv.fr/communes/{insee}")).unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("format", "json");
    query.append_pair("fields", "nom");
    drop(query);
    debug!("GET {}", url.as_str());
    let client = json_client();
    let request = client.get(url.as_str()).build().ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".insee_{insee}.json");
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
        serde_json::from_str::<City>(&text).ok().map(|it| it.name)
    }
    #[cfg(not(test))]
    response.json::<City>().await.ok().map(|it| it.name)
}

pub async fn alternate_city_names(insee_code: &str) -> Option<Vec<String>> {
    let mut url = Url::parse("https://geo.api.gouv.fr/communes_associees_deleguees").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("chefLieu", insee_code);
    drop(query);
    debug!("GET {}", url.as_str());
    let client = json_client();
    let request = client.get(url.as_str()).build().ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct Result {
        #[serde(rename = "nom")]
        name: String,
    }
    #[cfg(test)]
    let results = {
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".alternate_city_names_{insee_code}.json");
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
        serde_json::from_str::<Vec<Result>>(&text).ok()?
    };
    #[cfg(not(test))]
    let results = response.json::<Vec<Result>>().await.ok()?;
    Some(results.into_iter().map(|it| it.name).collect())
}

pub async fn cities_by_zip_code(zip_code: &str) -> Option<Vec<City>> {
    let mut url = Url::parse("https://geo.api.gouv.fr/communes").unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("codePostal", zip_code);
    drop(query);
    debug!("GET {}", url.as_str());
    let client = json_client();
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

#[allow(dead_code)]
async fn address(insee: Option<&str>, text: &str) -> Option<Vec<Address>> {
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
    let client = json_client();
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
    async fn test_search_by_insee_code() {
        let t0 = SystemTime::now();
        let name = city_name_by_insee("85092").await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!("Fontenay-le-Comte", name);
    }

    #[tokio::test]
    async fn test_search_by_zip_code() {
        let t0 = SystemTime::now();
        let results = cities_by_zip_code("85200").await.unwrap();
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

use crate::http_client::json_client;
use crate::myffme::license::deserialize_license_type;
use crate::myffme::{LicenseType, MYFFME_AUTHORIZATION};
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Debug, Deserialize)]
pub(crate) struct Product {
    pub id: String,
    #[serde(rename = "slug", deserialize_with = "deserialize_license_type")]
    pub license_type: LicenseType,
}

pub(crate) async fn products() -> Option<Vec<Product>> {
    let mut url = Url::parse("https://api.core.myffme.fr/api/products").unwrap();
    url.query_pairs_mut()
        .append_pair("pagination", "true")
        .append_pair("itemsPerPage", "500")
        .append_pair("page", "1");
    let client = json_client();
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
    let list = {
        println!("products");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".products.json";
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
        serde_json::from_str::<Vec<Value>>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
    };
    #[cfg(not(test))]
    let list = response
        .json::<Vec<Value>>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?;
    Some(
        list.into_iter()
            .filter_map(|value| serde_json::from_value::<Product>(value).ok())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_products() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let products = products().await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!("{}", products.len());
        println!("{products:?}");
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
                    .find(|it| it.license_type == license_type)
                    .is_some(),
                "{}",
                license_name
            );
        }
    }
}

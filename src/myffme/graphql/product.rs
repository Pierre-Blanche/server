use crate::http_client::json_client;
use crate::myffme::graphql::{ADMIN, X_HASURA_ROLE};
use crate::myffme::{LicenseType, MYFFME_AUTHORIZATION};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
pub(crate) struct Product {
    pub id: String,
    #[serde(deserialize_with = "deserialize_license_type")]
    pub license_type: Option<LicenseType>,
}

pub(crate) async fn products() -> Option<Vec<Product>> {
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
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(products)
}

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

pub(crate) fn deserialize_license_type<'de, D>(
    deserializer: D,
) -> Result<Option<LicenseType>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
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
}

use crate::address::City;
use crate::http_client::json_client;
use crate::myffme::licensee::{address, user_data};
use crate::myffme::MYFFME_AUTHORIZATION;
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tracing::warn;

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct Address {
    #[serde(skip_serializing)]
    pub user_id: Option<String>,
    #[serde(skip_serializing)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

#[derive(Deserialize)]
struct Addr {
    user_id: Option<String>,
    id: Option<String>,
    line1: Option<String>,
    line2: Option<String>,
    address: Option<String>,
    insee: Option<String>,
    #[serde(alias = "zipcode")]
    zip_code: Option<String>,
    city: Option<String>,
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let addr = Addr::deserialize(deserializer)?;
        if let Some(address) = addr.address {
            Ok(Address {
                user_id: addr.user_id,
                id: addr.id,
                address: Some(address),
                insee: addr.insee,
                zip_code: addr.zip_code,
                city: addr.city,
            })
        } else if let Some(line1) = addr.line1 {
            if let Some(line2) = addr.line2 {
                Ok(Address {
                    user_id: addr.user_id,
                    id: addr.id,
                    address: Some(format!("{} {}", line1, line2)),
                    insee: addr.insee,
                    zip_code: addr.zip_code,
                    city: addr.city,
                })
            } else {
                Ok(Address {
                    user_id: addr.user_id,
                    id: addr.id,
                    address: Some(line1),
                    insee: addr.insee,
                    zip_code: addr.zip_code,
                    city: addr.city,
                })
            }
        } else if let Some(line2) = addr.line2 {
            Ok(Address {
                user_id: addr.user_id,
                id: addr.id,
                address: Some(line2),
                insee: addr.insee,
                zip_code: addr.zip_code,
                city: addr.city,
            })
        } else {
            Ok(Address {
                user_id: addr.user_id,
                id: addr.id,
                address: None,
                insee: addr.insee,
                zip_code: addr.zip_code,
                city: addr.city,
            })
        }
    }
}

pub(crate) async fn user_address(myffme_user_id: &str) -> Option<Address> {
    let user_data = user_data(myffme_user_id).await?;
    let path = user_data.address_paths.and_then(|it| {
        let mut iter = it.into_iter();
        let found = iter.next();
        if iter.next().is_some() {
            warn!("more than one address found for user {}", myffme_user_id);
        }
        found
    })?;
    address(&path).await
}

pub(crate) async fn update_address_city(
    address_id: &str,
    city: &str,
    zip_code: &str,
) -> Option<()> {
    let url = Url::parse(&format!(
        "https://api.core.myffme.fr/api/addresses/{address_id}"
    ))
    .unwrap();
    let client = json_client();
    let response = client
        .patch(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "city": city,
            "zipcode": zip_code,
        }))
        .send()
        .await
        .ok()?;
    #[cfg(test)]
    let success = {
        println!("address city");
        println!("PATCH {}", url.as_str());
        let success = response.status().is_success();
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".update_address_city_{address_id}.json");
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
        success
    };
    #[cfg(not(test))]
    let success = response.status().is_success();
    if success {
        Some(())
    } else {
        warn!("failed to update address city");
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::myffme::address::Address;
    use serde_json::json;

    #[test]
    fn test_deserialize_address() {
        let value = "8-10 quai de la Marne";
        let json = json!({
            "line1": "8-10 quai de la Marne"
        });
        let address = serde_json::from_value::<Address>(json).unwrap();
        assert_eq!(value, &address.address.unwrap_or_default());
        let json = json!({
            "address": "8-10 quai de la Marne"
        });
        let address = serde_json::from_value::<Address>(json).unwrap();
        assert_eq!(value, &address.address.unwrap_or_default());
        let json = json!({
            "line1": "8-10",
            "line2": "quai de la Marne"
        });
        let address = serde_json::from_value::<Address>(json).unwrap();
        assert_eq!(value, &address.address.unwrap_or_default());
    }
}

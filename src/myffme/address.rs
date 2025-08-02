use crate::myffme::licensee::{address, user_data};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::warn;

#[derive(Debug, Serialize, Default)]
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
            warn!("More than one address found for user {}", myffme_user_id);
        }
        found
    })?;
    address(&path).await
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

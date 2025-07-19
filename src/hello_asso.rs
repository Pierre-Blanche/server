use crate::http_client::json_client;
use crate::myffme::address::{user_address, Address};
use crate::order::{Order, Priced};
use crate::season::is_during_discount_period;
use crate::user::Metadata;
use hyper::header::{HeaderValue, AUTHORIZATION};
use pinboard::Pinboard;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
use tiered_server::server::DOMAIN_APEX;
use tiered_server::store::Snapshot;
use tiered_server::user::User;
use tracing::debug;

pub(crate) struct Authorization {
    pub(crate) bearer_token: HeaderValue,
    pub(crate) refresh_token: String,
    pub(crate) timestamp: u32,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    refresh_token: String,
    // expires_in: u32,
    // token_type: String,
}

pub(crate) const HELLO_ASSO_AUTHORIZATION_VALIDITY_SECONDS: u32 = 1_200; // 20min

const CLIENT_ID_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "HELLO_ASSO_CLIENT_ID",
};

const CLIENT_SECRET_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "HELLO_ASSO_CLIENT_SECRET",
};

const OAUTH_ENDPOINT_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "HELLO_ASSO_OAUTH_ENDPOINT",
};

const API_ENDPOINT_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "HELLO_ASSO_API_ENDPOINT",
};

const ORG_SLUG_KEY: ConfigurationKey = ConfigurationKey::Other {
    variable_name: "HELLO_ASSO_ORG_SLUG",
};

static CLIENT_ID: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(CLIENT_ID_KEY).expect("hello asso client id not set"));

static CLIENT_SECRET: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(CLIENT_SECRET_KEY).expect("hello asso client secret not set"));

static OAUTH_ENDPOINT: LazyLock<&'static str> = LazyLock::new(|| {
    secret_value(OAUTH_ENDPOINT_KEY).unwrap_or("https://api.helloasso-sandbox.com/oauth2")
});

static API_ENDPOINT: LazyLock<&'static str> = LazyLock::new(|| {
    secret_value(API_ENDPOINT_KEY).unwrap_or("https://api.helloasso-sandbox.com/v5")
});

static ORG_SLUG: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(ORG_SLUG_KEY).expect("hello asso org slug not set"));

pub(crate) static HELLO_ASSO_AUTHORIZATION: LazyLock<Pinboard<Authorization>> =
    LazyLock::new(Pinboard::new_empty);

pub async fn update_hello_asso_bearer_token(timestamp: u32) -> Option<String> {
    let mut params = BTreeMap::new();
    let refresh_token = HELLO_ASSO_AUTHORIZATION
        .get_ref()
        .map(|it| it.refresh_token.clone());
    params.insert("client_id", *CLIENT_ID);
    params.insert("grant_type", "client_credentials");
    if let Some(ref refresh_token) = refresh_token {
        params.insert("refresh_token", refresh_token);
    } else {
        params.insert("client_secret", *CLIENT_SECRET);
    }
    let client = json_client();
    match client
        .post(format!("{}/token", *OAUTH_ENDPOINT))
        .form(&params)
        .send()
        .await
    {
        Ok(response) => match response.json::<Token>().await {
            Ok(token) => {
                let bearer_token =
                    HeaderValue::try_from(format!("Bearer {}", token.access_token)).unwrap();
                #[cfg(test)]
                println!("bearer token: {}", bearer_token.to_str().unwrap());
                let refresh_token = token.refresh_token;
                HELLO_ASSO_AUTHORIZATION.set(Authorization {
                    bearer_token,
                    refresh_token,
                    timestamp,
                });
                Some(token.access_token)
            }
            Err(err) => {
                debug!("failed to parse oauth2 response:\n{err:?}");
                None
            }
        },
        Err(err) => {
            debug!("failed to get oauth2 response:\n{err:?}");
            None
        }
    }
}

#[derive(Deserialize)]
pub struct Checkout {
    pub(crate) id: String,
    #[serde(rename = "redirectUrl")]
    pub(crate) redirect_url: String,
}

pub async fn init_transaction(snapshot: &Snapshot, user: &User, order: &Order) -> Option<Checkout> {
    let client = json_client();
    let price = order.price_in_cents(snapshot, is_during_discount_period(None));
    let return_url = format!("https://www.{}/user", *DOMAIN_APEX);
    let address = if let Some(ffme_id) = user
        .metadata
        .as_ref()
        .and_then(|it| Metadata::deserialize(it).ok())
        .and_then(|it| it.myffme_user_id)
    {
        user_address(&ffme_id).await.unwrap_or_default()
    } else {
        Address::default()
    };
    let dob_str = user.date_of_birth.to_string();
    match client
        .post(format!(
            "{}/organizations/{}/checkout-intents",
            *API_ENDPOINT, *ORG_SLUG
        ))
        .header(
            AUTHORIZATION,
            HELLO_ASSO_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "totalAmount": price,
            "initialAmount": price,
            "itemName": order.to_string(),
            "backUrl": &return_url,
            "returnUrl": &return_url,
            "errorUrl": &return_url,
            "containsDonation": false,
            "payer": {
                "firstName": &user.first_name,
                "lastName": &user.last_name,
                "email": &user.email(),
                "address": &address.line1,
                "city": &address.city,
                "zipCode": &address.zip_code,
                "country": "fra",
                "dateOfBirth": format!("{}-{}-{}", dob_str.get(..2).unwrap(), dob_str.get(2..4).unwrap(), dob_str.get(4..).unwrap()),
            }
        }))
        .send()
        .await
    {
        Ok(response) => {
            response.json().await.ok()?
        }
        Err(err) => {
            eprintln!("err: {err:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_update_hello_asso_bearer_token() {
        assert!(update_hello_asso_bearer_token(0).await.is_some());
    }
}

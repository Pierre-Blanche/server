use crate::http_client::json_client;
use crate::order::Order;
use hyper::header::HeaderValue;
use pinboard::Pinboard;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use tiered_server::env::{secret_value, ConfigurationKey};
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
    let mut client = json_client();
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

pub async fn init_transaction(user: &User, order: &Order) -> Option<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_update_hello_asso_bearer_token() {
        assert!(update_hello_asso_bearer_token(0).await.is_some());
    }
}

#![allow(dead_code)]

use crate::http_client::json_client;
use crate::myffme::licensee::UserData;
use crate::myffme::MYFFME_AUTHORIZATION;
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub(crate) async fn me() -> Option<UserData> {
    let url = Url::parse("https://api.core.myffme.fr/api/users/me").unwrap();
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
    let data = {
        println!("me");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".api/.me.json";
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
        serde_json::from_str::<Me>(&text)
            .inspect_err(|err| eprintln!("{err:?}"))
            .ok()?
    };
    #[cfg(not(test))]
    let data = response
        .json::<Me>()
        .await
        .inspect_err(|err| tracing::warn!("{err:?}"))
        .ok()?;
    Some(data.user_data)
}

#[derive(Debug, Deserialize)]
struct Me {
    #[serde(rename = "userData")]
    user_data: UserData,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;

    #[tokio::test]
    async fn test_me() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        me().await.unwrap();
    }
}

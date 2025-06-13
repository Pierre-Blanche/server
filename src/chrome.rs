use crate::http_client::json_client;
use pinboard::Pinboard;
use serde::Deserialize;
use std::sync::LazyLock;
use tracing::debug;

pub(crate) const USERAGENT_VALIDITY_SECONDS: u32 = 250_000; // ~3days

pub(crate) struct ChromeVersion {
    pub(crate) chrome_version: u16,
    pub(crate) timestamp: u32,
}

pub(crate) static CHROME_VERSION: LazyLock<Pinboard<ChromeVersion>> =
    LazyLock::new(Pinboard::new_empty);

#[derive(Deserialize)]
struct Release {
    milestone: u16,
}

pub(crate) async fn update_chrome_version(timestamp: u32) -> bool {
    match json_client()
        .get("https://chromiumdash.appspot.com/fetch_releases?channel=Stable&platform=Windows&num=1&offset=0")
        .send()
        .await
    {
        Ok(response) => match response.json::<Vec<Release>>().await {
            Ok(it) => {
                if it.is_empty() {
                    debug!("failed to get chrome version");
                    return false;
                }
                CHROME_VERSION.set(ChromeVersion {
                    chrome_version: it.into_iter().next().map(|it| it.milestone).unwrap(),
                    timestamp,
                });
                true
            }
            Err(err) => {
                debug!("failed to get chrome version:\n{err:?}");
                false
            }
        },
        Err(err) => {
            debug!("failed to get response from chromiumdash for the latest chrome version:\n{err:?}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_update_chrome_version() {
        assert!(update_chrome_version(0).await);
        println!(
            "latest stable chrome version: {}",
            CHROME_VERSION.get_ref().unwrap().chrome_version
        );
    }
}

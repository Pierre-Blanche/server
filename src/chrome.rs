use crate::http_client::json_client;
use pinboard::Pinboard;
use std::sync::LazyLock;
use tracing::debug;

pub(crate) const USERAGENT_VALIDITY_SECONDS: u32 = 250_000; // ~3days

pub(crate) struct ChromeVersion {
    pub(crate) chrome_version: u16,
    pub(crate) timestamp: u32,
}

pub(crate) static CHROME_VERSION: LazyLock<Pinboard<ChromeVersion>> =
    LazyLock::new(Pinboard::new_empty);

pub(crate) async fn update_chrome_version(timestamp: u32) -> bool {
    match json_client()
        .get("https://raw.githubusercontent.com/chromium/chromium/main/chrome/VERSION")
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(text) => {
                match text.lines().next().and_then(|it| {
                    let mut split = it.split('=');
                    let _ = split.next();
                    split.next().and_then(|it| it.parse::<u16>().ok())
                }) {
                    Some(chrome_version) => {
                        CHROME_VERSION.set(ChromeVersion {
                            chrome_version: chrome_version - 2,
                            timestamp,
                        });
                        true
                    }
                    None => {
                        debug!("failed to parse chrome version");
                        false
                    }
                }
            }
            Err(err) => {
                debug!("failed to get chrome verson file from github:\n{err:?}");
                false
            }
        },
        Err(err) => {
            debug!("failed to get response from github for the chrome verson file:\n{err:?}");
            false
        }
    }
}

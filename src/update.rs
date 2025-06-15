use crate::chrome::{update_chrome_version, CHROME_VERSION, USERAGENT_VALIDITY_SECONDS};
use crate::myffme::{
    update_myffme_bearer_token, MYFFME_AUTHORIZATION, MYFFME_AUTHORIZATION_VALIDITY_SECONDS,
};
use crate::order::update_prices;
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

pub async fn update_loop() {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    update_chrome_version(timestamp).await;
    let _ = update_myffme_bearer_token(timestamp).await;
    let _ = update_prices().await;
    thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async {
                loop {
                    let timestamp = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u32;
                    let chrome_version_timestamp =
                        CHROME_VERSION.get_ref().map(|it| it.timestamp).unwrap_or(0);
                    let mut success = true;
                    if timestamp > chrome_version_timestamp + USERAGENT_VALIDITY_SECONDS
                        && !update_chrome_version(timestamp).await
                    {
                        success = false;
                    }
                    let token_timestamp = MYFFME_AUTHORIZATION
                        .get_ref()
                        .map(|it| it.timestamp)
                        .unwrap_or(0);
                    if timestamp > token_timestamp + MYFFME_AUTHORIZATION_VALIDITY_SECONDS
                        && update_myffme_bearer_token(timestamp).await.is_none()
                    {
                        success = false;
                    }
                    sleep(Duration::from_secs(if success {
                        (15_000 + fastrand::i16(-1500..1500)) as u64
                    } else {
                        (600 + fastrand::i16(-100..100)) as u64
                    }))
                    .await;
                }
            })
    });
}

use crate::http_client::json_client;
use crate::myffme::MYFFME_AUTHORIZATION;
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde_json::json;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub(crate) async fn update_email(
    myffme_user_id: &str,
    email: &str,
    alt_email: Option<&str>,
) -> Option<()> {
    let url = Url::parse(&format!(
        "https://api.core.myffme.fr/api/user_datas/{myffme_user_id}"
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
            "email": email,
            "secondaryEmail": alt_email,
        }))
        .send()
        .await
        .ok()?;
    #[cfg(test)]
    let success = {
        println!("email");
        println!("PATCH {}", url.as_str());
        let success = response.status().is_success();
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".update_email_{myffme_user_id}.json");
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
        tracing::warn!("Failed to update email");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::me::me;
    use crate::myffme::update_myffme_bearer_token;

    #[tokio::test]
    async fn test_update_email() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        assert!(
            update_email("00000000-0000-0000-0000-000000000000", "", None)
                .await
                .is_none()
        );
        let user_data = me().await.unwrap();
        let email = user_data.email.unwrap();
        let alternate_email = user_data.alternate_email.as_ref().map(|it| it.as_str());
        assert!(
            update_email(&user_data.id, &email, Some(&email))
                .await
                .is_some()
        );
        assert!(
            update_email(&user_data.id, &email, alternate_email)
                .await
                .is_some()
        );
    }
}

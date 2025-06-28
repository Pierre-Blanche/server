use crate::http_client::json_client;
use crate::myffme::{ADMIN, MYFFME_AUTHORIZATION, X_HASURA_ROLE};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tracing::warn;

pub(crate) async fn update_email(
    user_id: &str,
    email: &str,
    alt_email: Option<&str>,
) -> Option<()> {
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
            "operationName": "updateEmail",
            "query": GRAPHQL_UPDATE_EMAIL,
            "variables": {
                "user_id": user_id,
                "email": email,
                "alt_email": alt_email,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    let success = response.status().is_success();
    if success {
        #[derive(Deserialize)]
        struct UserId {
            id: String,
        }
        #[derive(Deserialize)]
        struct MutationResult {
            result: Option<UserId>,
        }
        #[derive(Deserialize)]
        struct GraphqlResponse {
            data: MutationResult,
        }
        #[cfg(test)]
        let id = {
            println!("POST {}", url.as_str());
            println!("{}", response.status());
            let text = response.text().await.ok()?;
            let file_name = format!(".update_email_{user_id}.json");
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
                .map_err(|err| {
                    eprintln!("{err:?}");
                    err
                })
                .ok()?
                .data
                .result
                .map(|it| it.id)
        };
        #[cfg(not(test))]
        let id = response
            .json::<GraphqlResponse>()
            .await
            .map_err(|err| {
                warn!("{err:?}");
                err
            })
            .ok()?
            .data
            .result
            .map(|it| it.id);
        if let Some(ref id) = id {
            if id == user_id { Some(()) } else { None }
        } else {
            None
        }
    } else {
        None
    }
}

const GRAPHQL_UPDATE_EMAIL: &str = "\
    mutation updateEmail(
        $user_id: uuid!
        $email: String!
        $alt_email: String!
    ) {
        result: update_UTI_Utilisateurs_by_pk(
            pk_columns: { id: $user_id }
            _set: { CT_Email: $email, CT_Email2: $alt_email }
        ) {
            id
        }
    }\
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_update_email() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let result = update_email(
            "6692903b-8032-43ea-8cd9-530f14bf5324",
            "programingjd@gmail.com",
            None,
        )
        .await;
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(result.is_some());
    }
}

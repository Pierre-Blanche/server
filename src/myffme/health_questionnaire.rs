use crate::http_client::json_client;
use crate::myffme::document::Document;
use crate::myffme::{ADMIN, MYFFME_AUTHORIZATION, X_HASURA_ROLE};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use tokio::io::AsyncWriteExt;
use tracing::warn;

pub(crate) async fn user_health_questionnaires(
    ids: &[&str],
    season: u16,
) -> Option<BTreeMap<String, Document>> {
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
            "operationName": "getHealthQuestionnairesByUserIds",
            "query": GRAPHQL_GET_HEALTH_QUESTIONNAIRES_BY_USER_IDS,
            "variables": {
                "ids": ids,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct DocumentList {
        list: Vec<Document>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: DocumentList,
    }
    #[cfg(test)]
    let documents = {
        println!("health questionnaires");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".health_questionnaires.json");
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
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let documents = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        documents
            .into_iter()
            .map(|mut document| {
                let id = document.user_id.take().unwrap();
                (id, document)
            })
            .collect(),
    )
}

const GRAPHQL_GET_HEALTH_QUESTIONNAIRES_BY_USER_IDS: &str = "\
    query getHealthQuestionnairesByUserIds(
        $ids: [uuid!]!
        $season: Int!
    ) {
        list: DOC_Document(
            distinct_on: ID_Utilisateur
            order_by: [ { ID_Utilisateur: asc }, { ID_Saison: desc_nulls_last } ]
            where: {
                ID_Utilisateur: { _in: $ids }
                EST_DocumentValide: { _eq: true }
                EST_Actif: { _eq: true }
                ID_Type_Document: { _in: [ 60 ] }
                ID_Saison: { _lte: $season }
            }
        ) {
            user_id: ID_Utilisateur,
            season: ID_Saison,
            status,
            category: ID_Type_Document
        }
    }\
";

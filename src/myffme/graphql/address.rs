use crate::address::City;
use crate::http_client::json_client;
use crate::myffme::address::Address;
use crate::myffme::graphql::{ADMIN, X_HASURA_ROLE};
use crate::myffme::MYFFME_AUTHORIZATION;
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[allow(dead_code)]
pub(crate) async fn user_address(id: &str) -> Option<Address> {
    user_addresses([id].as_slice())
        .await
        .and_then(|mut it| it.remove(id))
}

pub(crate) async fn user_addresses(ids: &[&str]) -> Option<BTreeMap<String, Address>> {
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
            "operationName": "getAddressesByUserIds",
            "query": GRAPHQL_GET_ADDRESSES_BY_USER_IDS,
            "variables": {
                "ids": ids,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct AddressList {
        list: Vec<Address>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: AddressList,
    }
    #[cfg(test)]
    let addresses = {
        println!("addresses");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".addresses.json");
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
    let addresses = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    Some(
        addresses
            .into_iter()
            .map(|mut address| {
                let id = address.user_id.take().unwrap();
                (id, address)
            })
            .collect(),
    )
}

pub async fn update_address(
    user_id: &str,
    zip_code: &str,
    city: &City,
    line1: Option<&str>,
    country_id: Option<u16>,
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
            "operationName": "updateAddress",
            "query": GRAPHQL_UPDATE_ADDRESS_CITY,
            "variables": {
                "id": user_id,
                "city": city.name,
                "zip": zip_code,
                "insee": city.insee
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    let success = response.status().is_success();
    if success {
        #[derive(Deserialize)]
        struct AffectedRows {
            affected_rows: u16,
        }
        #[derive(Deserialize)]
        struct MutationResult {
            result: AffectedRows,
        }
        #[derive(Deserialize)]
        struct GraphqlResponse {
            data: MutationResult,
        }
        #[cfg(test)]
        let affected_rows = {
            println!("POST {}", url.as_str());
            println!("{}", response.status());
            let text = response.text().await.ok()?;
            let file_name = format!(".update_address_{user_id}.json");
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
                .affected_rows
        };
        #[cfg(not(test))]
        let affected_rows = response
            .json::<GraphqlResponse>()
            .await
            .map_err(|err| {
                tracing::warn!("{err:?}");
                err
            })
            .ok()?
            .data
            .result
            .affected_rows;
        if affected_rows > 0 {
            Some(())
        } else {
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
                    "operationName": "insertAddress",
                    "query": GRAPHQL_INSERT_ADDRESS_CITY,
                    "variables": {
                        "id": user_id,
                        "city": city.name,
                        "zip": zip_code,
                        "insee": city.insee,
                        "line1": line1.unwrap_or_default(),
                        "country_id": country_id.unwrap_or(75)
                    }
                }))
                .build()
                .ok()?;
            let response = client.execute(request).await.ok()?;
            let success = response.status().is_success();
            #[cfg(test)]
            {
                println!("POST {}", url.as_str());
                println!("{}", response.status());
                let text = response.text().await.ok()?;
                let file_name = format!(".insert_address_{user_id}.json");
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
            }
            if success { Some(()) } else { None }
        }
    } else {
        None
    }
}

const GRAPHQL_GET_ADDRESSES_BY_USER_IDS: &str = "\
    query getAddressesByUserIds(
        $ids: [uuid!]!
    ) {
        list: ADR_Adresse(
            where: { ID_Utilisateur: { _in: $ids } }
            order_by: [ { ID_Utilisateur: asc }, { Z_DateModification: desc } ]
            distinct_on: [ ID_Utilisateur ]
        ) {
            user_id: ID_Utilisateur
            line1: Adresse1
            line2: Adresse2
            insee: CodeInsee
            zip_code: CodePostal,
            city: Ville
        }
    }\
";

const GRAPHQL_UPDATE_ADDRESS_CITY: &str = "\
    mutation updateAddress(
        $id: uuid!
        $city: String!
        $zip: String!
        $insee: String!
    ) {
        result: update_ADR_Adresse(
            where: { ID_Utilisateur: { _eq: $id } }
            _set: {
                Ville: $city
                CodeInsee: $insee
                CodePostal: $zip
                # ID_Pays: 75
            }
        ) {
            affected_rows
        }
    }\
";

const GRAPHQL_INSERT_ADDRESS_CITY: &str = "\
    mutation insertAddress(
        $id: uuid!
        $city: String!
        $zip: String!
        $insee: String!
        $line1: String!
        $country_id: Int!
    ) {
        result: insert_ADR_Adresse_one(
            object: {
                ID_Utilisateur: $id
                Ville: $city
                CodeInsee: $insee
                CodePostal: $zip
                Adresse1: $line1
                ID_Pays: $country_id
            }
        ) {
            id
        }
    }\
";

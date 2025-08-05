use crate::http_client::json_client;
use crate::myffme::graphql::{ADMIN, X_HASURA_ROLE};
use crate::myffme::{License, Structure, MYFFME_AUTHORIZATION};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub(crate) async fn structures_by_ids(ids: &[u32]) -> Option<BTreeMap<u32, Structure>> {
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
            "operationName": "getStructuresByIds",
            "query": GRAPHQL_GET_STRUCTURES_BY_IDS,
            "variables": {
                "ids": ids,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct StructureList {
        list: Vec<Structure>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: StructureList,
    }
    #[cfg(test)]
    let structures = {
        println!("structures");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".graphql/.structures.json");
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
            .inspect_err(|err| eprintln!("{err:?}"))
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let structures = response
        .json::<GraphqlResponse>()
        .await
        .inspect_err(|err| tracing::warn!("{err:?}"))
        .ok()?
        .data
        .list;
    Some(
        structures
            .into_iter()
            .map(|structure| {
                let id = structure.id;
                (id, structure)
            })
            .collect(),
    )
}

pub(crate) async fn structure_licenses(
    structure_id: u32,
    season: u16,
) -> Option<BTreeMap<String, License>> {
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
            "operationName": "getLicensesByStructureIdAndSeason",
            "query": GRAPHQL_GET_LICENSES_BY_STRUCTURE_ID_AND_SEASON,
            "variables": {
                "structure_id": structure_id,
                "season": season,
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct LicenseList {
        list: Vec<License>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: LicenseList,
    }
    #[cfg(test)]
    let licenses = {
        println!("licenses");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".graphql/.licenses.json");
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
            .inspect_err(|err| eprintln!("{err:?}"))
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let licenses = response
        .json::<GraphqlResponse>()
        .await
        .inspect_err(|err| tracing::warn!("{err:?}"))
        .ok()?
        .data
        .list;
    Some(
        licenses
            .into_iter()
            .map(|mut license| {
                let id = license.user_id.take().unwrap();
                (id, license)
            })
            .collect(),
    )
}

#[derive(Deserialize)]
pub(crate) struct StructureHierarchy {
    #[allow(dead_code)]
    pub id: u32,
    pub department_structure_id: u32,
    pub region_structure_id: u32,
    pub national_structure_id: u32,
}

pub(crate) async fn structure_hierarchy_by_id(id: u32) -> Option<StructureHierarchy> {
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
            "operationName": "getStructuresByIds",
            "query": GRAPHQL_GET_STRUCTURES_BY_IDS,
            "variables": {
                "ids": [id],
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct StructureList {
        list: Vec<StructureHierarchy>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: StructureList,
    }
    #[cfg(test)]
    let structure_hierarchy = {
        println!("structure");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".graphql/.structure.json");
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
            .inspect_err(|err| eprintln!("{err:?}"))
            .ok()?
            .data
            .list
            .into_iter()
            .next()?
    };
    #[cfg(not(test))]
    let structure_hierarchy = response
        .json::<GraphqlResponse>()
        .await
        .inspect_err(|err| tracing::warn!("{err:?}"))
        .ok()?
        .data
        .list
        .into_iter()
        .next()?;
    Some(structure_hierarchy)
}

const GRAPHQL_GET_STRUCTURES_BY_IDS: &str = "\
    query getStructuresByIds($ids: [Int!]!) {
        list: structure(
            where: { id: { _in: $ids } }
        ) {
            id
            code: federal_code
            name: label
            department: department_id
            department_structure_id: ct_id
            region_structure_id: ligue_id
            national_structure_id: ffme_id
        }
    }\
";

const GRAPHQL_GET_LICENSES_BY_STRUCTURE_ID_AND_SEASON: &str = "\
    query getLicensesByStructureIdAndSeason(
        $structure_id: Int!
        $season: Int!
    ) {
        list: licence(
            where: { structure_id: { _eq: $structure_id }, season_id: { _eq: $season } }
        ) {
            product_id
            non_practicing
            structure_id
            status
            user_id
            season: season_id
        }
    }\
";

#[cfg(test)]
mod tests {
    use crate::myffme::{update_myffme_bearer_token, STRUCTURE_ID};

    #[tokio::test]
    async fn test_structure_hierarchy() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let hierarchy = structure_hierarchy_by_id(*STRUCTURE_ID).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!(
            "{}, {}, {}, {}",
            hierarchy.id,
            hierarchy.department_structure_id,
            hierarchy.region_structure_id,
            hierarchy.national_structure_id
        );
    }
}

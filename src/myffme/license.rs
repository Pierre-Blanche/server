use crate::http_client::json_client;
use crate::myffme::{ADMIN, MYFFME_AUTHORIZATION, X_HASURA_ROLE};
use crate::user::LicenseType;
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
pub(crate) struct License {
    pub user_id: Option<String>,
    pub season: u16,
    // #[serde(deserialize_with = "deserialize_license_number")]
    // pub license_number: u32,
    pub structure_id: u32,
    #[serde(rename = "product_id", deserialize_with = "deserialize_license_type")]
    pub license_type: Option<LicenseType>,
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct LicenseFees {
    pub federal_fee_in_cents: u16,
    pub regional_fee_in_cents: u16,
    pub department_fee_in_cents: u16,
}

impl TryFrom<&str> for LicenseType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "adult" | "licence_adulte" | "ab229bd0-53c7-4c8c-83d1-bade2cbb5fcc" => {
                Ok(LicenseType::Adult)
            }
            "non_member_adult" | "hors_club_adulte" | "8dd8c63f-a9da-4237-aec9-74f905fb2b37" => {
                Ok(LicenseType::NonMemberAdult)
            }
            "child" | "licence_jeune" | "09fd57d3-0f38-407d-95b5-08d3e8369297" => {
                Ok(LicenseType::Child)
            }
            "non_member_child" | "hors_club_jeune" | "46786452-7ca2-4dc1-a15d-effb3f7e69b0" => {
                Ok(LicenseType::NonMemberChild)
            }
            "family" | "licence_famille" | "865d950e-9825-49f3-858b-ca1a776734b3" => {
                Ok(LicenseType::Family)
            }
            "non_practicing" => Ok(LicenseType::NonPracticing),
            other => Err(format!("unknown license type: {other}")),
        }
    }
}

pub(crate) fn deserialize_license_type<'de, D>(
    deserializer: D,
) -> Result<Option<LicenseType>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
}

pub(crate) async fn user_licenses(ids: &[&str], season: u16) -> Option<BTreeMap<String, License>> {
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
            "operationName": "getLicensesByUserIds",
            "query": GRAPHQL_GET_LICENSES_BY_USER_IDS,
            "variables": {
                "ids": ids,
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
        let file_name = format!(".licenses.json");
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
    let licenses = response
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
        licenses
            .into_iter()
            .map(|mut license| {
                let id = license.user_id.take().unwrap();
                (id, license)
            })
            .collect(),
    )
}

const GRAPHQL_GET_LICENSES_BY_USER_IDS: &str = "\
    query getLicensesByUserIds(
        $ids: [uuid!]!
        $season: Int!
    ) {
        list: licence(
            where: { user_id: { _in: $ids }, season_id: { _lte: $season } }
            order_by: [ { user_id: asc }, { season_id: desc_nulls_last } ]
            distinct_on: user_id
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

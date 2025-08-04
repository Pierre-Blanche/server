use crate::http_client::json_client;
use crate::myffme::graphql::{ADMIN, X_HASURA_ROLE};
use crate::myffme::{InsuranceLevelOption, InsuranceOptionOption, MYFFME_AUTHORIZATION};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub(crate) async fn options() -> Option<(Vec<InsuranceLevelOption>, Vec<InsuranceOptionOption>)> {
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
            "operationName": "getOptions",
            "query": GRAPHQL_GET_OPTIONS,
            "variables": {}
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct OptionList {
        levels: Vec<InsuranceLevelOption>,
        options: Vec<InsuranceOptionOption>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: OptionList,
    }
    #[cfg(test)]
    let options = {
        println!("options");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".options.json");
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
    };
    #[cfg(not(test))]
    let options = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data;
    Some((options.levels, options.options))
}

// OptionType {
//     id: "0bd82f7a-8aa1-4aa7-80e9-43e32a37f829",
//     slug: "assurance"
// }
// OptionType {
//     id: "7912cb1c-b5e1-4e21-8195-1ec2573fb609",
//     slug: "option_assurance"
// }
const GRAPHQL_GET_OPTIONS: &str = "\
    query getOptions {
        levels: option(
            where: {
                option_type_id: { _eq: \"0bd82f7a-8aa1-4aa7-80e9-43e32a37f829\" }
            }
        ) {
            id
            level: slug
        }
        options: option(
            where: {
                option_type_id: { _eq: \"7912cb1c-b5e1-4e21-8195-1ec2573fb609\" }
            }
        ) {
            id
            option: slug
        }
    }\
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use crate::order::{InsuranceLevel, InsuranceOption};
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_options() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let (insurance_levels, insurance_options) = options().await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        for (insurance_level, level_name) in [
            (InsuranceLevel::RC, "RC"),
            (InsuranceLevel::Base, "Base"),
            (InsuranceLevel::BasePlus, "Base+"),
            (InsuranceLevel::BasePlusPlus, "Base++"),
        ] {
            assert!(
                insurance_levels
                    .iter()
                    .find(|it| it.level == insurance_level)
                    .is_some(),
                "{}",
                level_name
            );
        }
        for (insurance_option, option_name) in [
            (InsuranceOption::MountainBike, "Mountain Bike"),
            (InsuranceOption::Ski, "Ski"),
            (InsuranceOption::SlacklineAndHighline, "Slackline/Highline"),
            (InsuranceOption::TrailRunning, "Trail Running"),
        ] {
            assert!(
                insurance_options
                    .iter()
                    .find(|it| it.option == insurance_option)
                    .is_some(),
                "{}",
                option_name
            );
        }
    }
}

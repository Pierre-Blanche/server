use crate::http_client::json_client;
use crate::myffme::{ADMIN, MYFFME_AUTHORIZATION, X_HASURA_ROLE};
use crate::order::{InsuranceLevel, InsuranceOption};
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
pub(crate) struct InsuranceLevelOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_level")]
    pub level: Option<InsuranceLevel>,
}

#[derive(Deserialize)]
pub(crate) struct InsuranceOptionOption {
    pub id: String,
    #[serde(deserialize_with = "deserialize_insurance_option")]
    pub option: Option<InsuranceOption>,
}

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

impl TryFrom<&str> for InsuranceLevel {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "rc" | "Rc" | "RC" | "8e1b2635-a76a-40a4-a278-2cd6768d03c0" => Ok(InsuranceLevel::RC),
            "base" | "Base" | "4061064e-4d0a-4c49-9c66-109960a0437a" => Ok(InsuranceLevel::Base),
            "base_plus" | "BasePlus" | "a3a2d318-c8a5-410b-ac9d-1f07c1d69bdc" => {
                Ok(InsuranceLevel::BasePlus)
            }
            "base_plus_plus" | "BasePlusPlus" | "902fb734-a182-419a-af61-008b8bff3a4a" => {
                Ok(InsuranceLevel::BasePlusPlus)
            }
            other => Err(format!("unknown insurance level: {other}")),
        }
    }
}

impl TryFrom<&str> for InsuranceOption {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "vtt" | "MountainBike" | "mountain_bike" | "5e6eb7ec-7dc6-445b-ab50-9b45cb202f1e" => {
                Ok(InsuranceOption::MountainBike)
            }
            "ski_piste" | "Ski" | "ski" | "92e7eebe-71cd-4258-b178-141587374b81" => {
                Ok(InsuranceOption::Ski)
            }
            "slackline_highline"
            | "SlacklineAndHighline"
            | "slackline_and_highline"
            | "dae0654d-977c-46c5-8f48-63de2d127efd" => Ok(InsuranceOption::SlacklineAndHighline),
            "trail" | "TrialRunning" | "trial_running" | "d9c13113-70eb-4e04-a265-aba8f8ea7e8b" => {
                Ok(InsuranceOption::TrailRunning)
            }
            other => Err(format!("unknown insurance option: {other}")),
        }
    }
}

fn deserialize_insurance_level<'de, D>(deserializer: D) -> Result<Option<InsuranceLevel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
}

fn deserialize_insurance_option<'de, D>(
    deserializer: D,
) -> Result<Option<InsuranceOption>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let result = <&str>::deserialize(deserializer);
    match result {
        Ok(str) => Ok(str.try_into().ok()),
        Err(_err) => Ok(None),
    }
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
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_options() {
        assert!(update_myffme_bearer_token(0).await.is_some());
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
                    .find(|it| it.level.as_ref() == Some(&insurance_level))
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
                    .find(|it| it.option.as_ref() == Some(&insurance_option))
                    .is_some(),
                "{}",
                option_name
            );
        }
    }
}

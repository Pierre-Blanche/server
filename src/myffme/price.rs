use crate::http_client::json_client;
use crate::myffme::license::LicenseFees;
use crate::myffme::options::options;
use crate::myffme::product::products;
use crate::myffme::structure::{structure_hierarchy_by_id, StructureHierarchy};
use crate::myffme::{ADMIN, MYFFME_AUTHORIZATION, STRUCTURE_ID, X_HASURA_ROLE};
use crate::order::{InsuranceLevel, InsuranceOption};
use crate::season::current_season;
use crate::user::LicenseType;
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub(crate) async fn prices(
    season: Option<u16>,
) -> Option<(
    BTreeMap<LicenseType, LicenseFees>,
    BTreeMap<InsuranceLevel, u16>,
    BTreeMap<InsuranceOption, u16>,
)> {
    let StructureHierarchy {
        department_structure_id,
        region_structure_id,
        national_structure_id,
        ..
    } = structure_hierarchy_by_id(*STRUCTURE_ID).await?;
    let products = products().await?;
    let (levels, options) = options().await?;
    let mut levels = levels
        .into_iter()
        .filter_map(|it| it.level.map(|level| (it.id, level)))
        .collect::<BTreeMap<_, _>>();
    let mut options = options
        .into_iter()
        .filter_map(|it| it.option.map(|option| (it.id, option)))
        .collect::<BTreeMap<_, _>>();
    let product_ids = products.iter().map(|it| it.id.as_str()).collect::<Vec<_>>();
    let level_ids = levels.keys().collect::<Vec<_>>();
    let option_ids = options.keys().collect::<Vec<_>>();
    let season = season.unwrap_or(current_season(None));
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
            "operationName": "getPrices",
            "query": GRAPHQL_GET_PRICES,
            "variables": {
                "products": product_ids,
                "levels": level_ids,
                "options": option_ids,
                "department_structure_id": department_structure_id,
                "region_structure_id": region_structure_id,
                "national_structure_id": national_structure_id,
                "season": season
            }
        }))
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[derive(Deserialize)]
    struct Product {
        product_id: String,
        structure_id: u32,
        price_in_cents: u16,
    }
    #[derive(Deserialize)]
    struct LevelOrOption {
        option_id: String,
        price_in_cents: u16,
    }

    #[derive(Deserialize)]
    struct PriceList {
        products: Vec<Product>,
        levels: Vec<LevelOrOption>,
        options: Vec<LevelOrOption>,
    }
    #[derive(Deserialize)]
    struct GraphqlResponse {
        data: PriceList,
    }
    #[cfg(test)]
    let PriceList {
        products: product_list,
        levels: level_list,
        options: option_list,
    } = {
        println!("prices");
        println!("POST {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".prices.json");
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
    let PriceList {
        products: product_list,
        levels: level_list,
        options: option_list,
    } = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data;
    let mut license_prices = BTreeMap::new();
    for price in product_list.into_iter() {
        if let Some(license_type) = products
            .iter()
            .find(|it| it.id == price.product_id)
            .and_then(|it| it.license_type)
        {
            let fees: &mut LicenseFees = license_prices.entry(license_type).or_default();
            if price.structure_id == department_structure_id {
                fees.department_fee_in_cents = price.price_in_cents;
            } else if price.structure_id == region_structure_id {
                fees.regional_fee_in_cents = price.price_in_cents;
            } else if price.structure_id == national_structure_id {
                fees.federal_fee_in_cents = price.price_in_cents;
            }
        }
    }
    let mut level_prices = BTreeMap::new();
    for price in level_list.into_iter() {
        if let Some(level) = levels.remove(&price.option_id) {
            level_prices.insert(level, price.price_in_cents);
        }
    }
    let mut option_prices = BTreeMap::new();
    for price in option_list.into_iter() {
        if let Some(option) = options.remove(&price.option_id) {
            option_prices.insert(option, price.price_in_cents);
        }
    }
    Some((license_prices, level_prices, option_prices))
}

const GRAPHQL_GET_PRICES: &str = "\
    query getPrices(
        $products: [uuid!]!
        $levels: [uuid!]!
        $options: [uuid!]!
        $department_structure_id: Int!
        $region_structure_id: Int!
        $national_structure_id: Int!
        $season: Int!
    ) {
        products: price(
            where: {
                season_id: { _eq: $season }
                product_id: { _in: $products }
                structure_id: { _in: [ $department_structure_id, $region_structure_id, $national_structure_id ] }
                option_id: { _is_null: true }
            }
        ) {
            product_id
            structure_id
            price_in_cents: value
        }
        levels: price(
            where: {
                season_id: { _eq: $season }
                option_id: { _in: $levels }
                structure_id: { _eq: $national_structure_id }
                product_id: { _is_null: true }
            }
        ) {
            option_id
            price_in_cents: value
        }
        options: price(
            where: {
                season_id: { _eq: $season }
                option_id: { _in: $options }
                structure_id: { _eq: $national_structure_id }
                product_id: { _is_null: true }
            }
        ) {
            option_id
            price_in_cents: value
        }
    }\
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_prices() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let (license_prices, insurance_level_prices, insurance_option_prices) =
            prices(None).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        for (license_type, license_name) in [
            (LicenseType::Adult, "Adult"),
            (LicenseType::Child, "Child"),
            (LicenseType::Family, "Family"),
            (LicenseType::NonMemberAdult, "Non Member Adult"),
            (LicenseType::NonMemberChild, "Non Member Child"),
        ] {
            assert!(
                license_prices.get(&license_type).is_some(),
                "{}",
                license_name
            );
        }
        for (insurance_level, level_name) in [
            (InsuranceLevel::RC, "RC"),
            (InsuranceLevel::Base, "Base"),
            (InsuranceLevel::BasePlus, "Base+"),
            (InsuranceLevel::BasePlusPlus, "Base++"),
        ] {
            assert!(
                insurance_level_prices.get(&insurance_level).is_some(),
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
                insurance_option_prices.get(&insurance_option).is_some(),
                "{}",
                option_name
            );
        }
    }
}

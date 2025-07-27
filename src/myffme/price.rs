use crate::http_client::json_client;
use crate::myffme::license::{deserialize_product_option, ProductOption};
use crate::myffme::product::{products, Product};
use crate::myffme::{LicenseFees, LicenseType, MYFFME_AUTHORIZATION, STRUCTURE_ID};
use crate::order::{InsuranceLevel, InsuranceOption};
use crate::season::current_season;
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::de::Error;
use serde::Deserialize;
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
    let season = season.unwrap_or_else(|| current_season(None));
    let products = products().await?;
    let product_prices = product_prices(&products, season).await?;
    let (level_prices, option_prices) = insurance_prices(season).await?;
    Some((product_prices, level_prices, option_prices))
}

pub enum Fee {
    Department,
    Regional,
    Federal,
}

impl TryFrom<&str> for Fee {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "cotisation_ct" => Ok(Fee::Department),
            "cotisation_ligue" => Ok(Fee::Regional),
            "principal" => Ok(Fee::Federal),
            other => Err(format!("unknown fee kind: {other}")),
        }
    }
}

struct FeeVisitor;

impl<'de> serde::de::Visitor<'de> for FeeVisitor {
    type Value = Fee;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing an insurance option")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Fee::try_from(v).map_err(|err| E::custom(err))
    }
}

pub(crate) fn deserialize_fee<'de, D>(deserializer: D) -> Result<Fee, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(FeeVisitor)
}

#[derive(Deserialize)]
struct ProductPrice {
    #[serde(rename = "value")]
    price: u16,
    product: Product,
    #[serde(rename = "type", deserialize_with = "deserialize_fee")]
    fee: Fee,
}

async fn product_prices(
    products: &[Product],
    season: u16,
) -> Option<BTreeMap<LicenseType, LicenseFees>> {
    let mut iter = products.iter().map(|it| it.id.as_str()).peekable();
    let mut joined_results = String::new();
    while let Some(product_id) = iter.next() {
        joined_results.push_str(product_id);
        if iter.peek().is_some() {
            joined_results.push(',');
        }
    }
    let mut url = Url::parse("https://api.core.myffme.fr/api/prices").unwrap();
    url.query_pairs_mut()
        .append_pair("pagination", "true")
        .append_pair("itemsPerPage", "500")
        .append_pair("page", "1")
        .append_pair("seasonId", &season.to_string())
        .append_pair("productId", &joined_results)
        .append_pair("structureId", &STRUCTURE_ID.to_string());
    let client = json_client();
    let request = client
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    let list = {
        println!("license_prices");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".license_prices.json";
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<Vec<ProductPrice>>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
    };
    #[cfg(not(test))]
    let list = response
        .json::<Vec<ProductPrice>>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?;
    let mut license_prices: BTreeMap<LicenseType, LicenseFees> = BTreeMap::new();
    for price in list.into_iter() {
        let fee = license_prices
            .entry(price.product.license_type)
            .or_default();
        match price.fee {
            Fee::Department => fee.department_fee_in_cents = price.price,
            Fee::Regional => fee.regional_fee_in_cents = price.price,
            Fee::Federal => fee.federal_fee_in_cents = price.price,
        }
    }
    Some(license_prices)
}

#[derive(Deserialize)]
struct OptionPrice {
    #[serde(rename = "value")]
    price: u16,
    #[serde(rename = "option", deserialize_with = "deserialize_product_option")]
    product_option: ProductOption,
}

async fn insurance_prices(
    season: u16,
) -> Option<(
    BTreeMap<InsuranceLevel, u16>,
    BTreeMap<InsuranceOption, u16>,
)> {
    let mut url = Url::parse("https://api.core.myffme.fr/api/prices").unwrap();
    url.query_pairs_mut()
        .append_pair("pagination", "true")
        .append_pair("itemsPerPage", "500")
        .append_pair("page", "1")
        .append_pair("seasonId", &season.to_string())
        .append_pair(
            "options",
            "rc,base,base_plus,base_plus_plus,vtt,ski_piste,slackline_highline,trail",
        )
        .append_pair("structureId", &STRUCTURE_ID.to_string());
    let client = json_client();
    let request = client
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;
    #[cfg(test)]
    let list = {
        println!("insurance_prices");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".insurance_prices.json";
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<Vec<OptionPrice>>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
    };
    #[cfg(not(test))]
    let list = response
        .json::<Vec<OptionPrice>>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?;
    let mut level_prices = BTreeMap::new();
    let mut option_prices = BTreeMap::new();
    for price in list.into_iter() {
        match price.product_option {
            ProductOption::InsuranceLevel(level) => {
                level_prices.insert(level.level, price.price);
            }
            ProductOption::InsuranceOption(option) => {
                option_prices.insert(option.option, price.price);
            }
        }
    }
    Some((level_prices, option_prices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_product_prices() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let products = products().await.unwrap();
        let product_prices = product_prices(&products, current_season(None))
            .await
            .unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!("{}", product_prices.len());
        println!("{product_prices:?}");
        for (license_type, license_name) in [
            (LicenseType::Adult, "Adult"),
            (LicenseType::Child, "Child"),
            (LicenseType::Family, "Family"),
            (LicenseType::NonMemberAdult, "Non Member Adult"),
            (LicenseType::NonMemberChild, "Non Member Child"),
        ] {
            assert!(
                product_prices
                    .keys()
                    .find(|&it| it == &license_type)
                    .is_some(),
                "{}",
                license_name
            );
        }
    }

    #[tokio::test]
    async fn test_insurance_prices() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let (level_prices, option_prices) = insurance_prices(current_season(None)).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!("{}", level_prices.len());
        println!("{level_prices:?}");
        for (insurance_level, level_name) in [
            (InsuranceLevel::RC, "RC"),
            (InsuranceLevel::Base, "Base"),
            (InsuranceLevel::BasePlus, "Base+"),
            (InsuranceLevel::BasePlusPlus, "Base++"),
        ] {
            assert!(
                level_prices
                    .keys()
                    .find(|&it| it == &insurance_level)
                    .is_some(),
                "{}",
                level_name
            );
        }
        println!("{}", option_prices.len());
        println!("{option_prices:?}");
        for (insurance_option, option_name) in [
            (InsuranceOption::MountainBike, "Mountain Bike"),
            (InsuranceOption::Ski, "Ski"),
            (InsuranceOption::SlacklineAndHighline, "Slackline/Highline"),
            (InsuranceOption::TrailRunning, "Trail Running"),
        ] {
            assert!(
                option_prices
                    .keys()
                    .find(|&it| it == &insurance_option)
                    .is_some(),
                "{}",
                option_name
            );
        }
    }
}

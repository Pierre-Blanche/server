use crate::http_client::json_client;
use crate::myffme::license::{deserialize_license_type, deserialize_product_option, ProductOption};
use crate::myffme::{
    InsuranceLevelOption, InsuranceOptionOption, LicenseType, MYFFME_AUTHORIZATION,
};
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub(crate) struct Product {
    pub id: String,
    pub license_type: LicenseType,
}

pub(crate) async fn products() -> Option<(
    Vec<Product>,
    Vec<InsuranceLevelOption>,
    Vec<InsuranceOptionOption>,
)> {
    let mut url = Url::parse("https://api.core.myffme.fr/api/products").unwrap();
    url.query_pairs_mut()
        .append_pair("pagination", "true")
        .append_pair("itemsPerPage", "500")
        .append_pair("page", "1");
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
        println!("products");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".products.json");
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
        serde_json::from_str::<Vec<Value>>(&text)
            .map_err(|e| {
                eprintln!("{e:?}");
                e
            })
            .ok()?
    };
    #[cfg(not(test))]
    let list = response
        .json::<Vec<Value>>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?;
    let mut products = vec![];
    let mut insurance_level_options = vec![];
    let mut insurance_option_options = vec![];
    for product in list.into_iter() {
        if let Ok(product) = serde_json::from_value::<ProductWithOptions>(product) {
            let (product, option_types) = product.into();
            for value in option_types.into_iter().flat_map(|it| it.options) {
                match serde_json::from_value::<ProductOptionWrapper>(value)
                    .map(|it| it.product_option)
                {
                    Ok(ProductOption::InsuranceLevel(it)) => {
                        if !insurance_level_options
                            .iter()
                            .any(|it: &InsuranceLevelOption| it.id == it.id)
                        {
                            insurance_level_options.push(it);
                        }
                    }
                    Ok(ProductOption::InsuranceOption(it)) => {
                        if !insurance_option_options
                            .iter()
                            .any(|it: &InsuranceOptionOption| it.id == it.id)
                        {
                            insurance_option_options.push(it);
                        }
                    }
                    Err(_) => {}
                }
            }
            products.push(product);
        }
    }
    Some((products, insurance_level_options, insurance_option_options))
}

#[derive(Deserialize)]
struct OptionType {
    pub options: Vec<Value>,
}

#[derive(Deserialize)]
struct ProductWithOptions {
    pub id: String,
    #[serde(alias = "slug", deserialize_with = "deserialize_license_type")]
    pub license_type: LicenseType,
    #[serde(alias = "optionTypes")]
    pub option_types: Vec<OptionType>,
}

#[derive(Deserialize)]
struct ProductOptionWrapper {
    #[serde(flatten, deserialize_with = "deserialize_product_option")]
    pub product_option: ProductOption,
}

impl From<ProductWithOptions> for (Product, Vec<OptionType>) {
    fn from(product: ProductWithOptions) -> Self {
        (
            Product {
                id: product.id,
                license_type: product.license_type,
            },
            product.option_types,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use crate::order::{InsuranceLevel, InsuranceOption};
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_products() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let (products, insurance_levels, insurance_options) = products().await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!("{}", products.len());
        println!("{products:?}");
        for (license_type, license_name) in [
            (LicenseType::Adult, "Adult"),
            (LicenseType::Child, "Child"),
            (LicenseType::Family, "Family"),
            (LicenseType::NonMemberAdult, "Non Member Adult"),
            (LicenseType::NonMemberChild, "Non Member Child"),
        ] {
            assert!(
                products
                    .iter()
                    .find(|it| it.license_type == license_type)
                    .is_some(),
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

    #[test]
    fn test() {
        let result = serde_json::from_value::<ProductWithOptions>(serde_json::json!({
            "id": "123",
            "label": "lbl",
            "slug": "licence_jeune",
            "active": true,
            "optionTypes": []
        }))
        .unwrap();
        println!("{:?}", result.license_type);
    }
}

use crate::http_client::json_client;
use crate::myffme::license::deserialize_license_type;
use crate::myffme::{LicenseType, MedicalCertificateStatus, MYFFME_AUTHORIZATION, STRUCTURE_ID};
use crate::season::current_season;
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::de::Error;
use serde::Deserialize;
use std::collections::BTreeSet;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

impl TryFrom<&str> for MedicalCertificateStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "loisir" => Ok(MedicalCertificateStatus::Recreational),
            "competition" => Ok(MedicalCertificateStatus::Competition),
            "qs" => Ok(MedicalCertificateStatus::HealthQuestionnaire),
            "waiting_document" | "waiting_validation" | "validate" => {
                Ok(MedicalCertificateStatus::WaitingForDocument)
            }
            other => Err(format!("unknown insurance level: {other}")),
        }
    }
}

struct MedicalCertificateStatusVisitor;

impl<'de> serde::de::Visitor<'de> for MedicalCertificateStatusVisitor {
    type Value = MedicalCertificateStatus;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing a medical certificate status")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        MedicalCertificateStatus::try_from(v).map_err(|err| E::custom(err))
    }
}

pub(crate) fn deserialize_medical_certificate_status<'de, D>(
    deserializer: D,
) -> Result<MedicalCertificateStatus, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(MedicalCertificateStatusVisitor)
}

struct DateVisitor;

impl<'de> serde::de::Visitor<'de> for DateVisitor {
    type Value = u32;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing a date")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        let date = v
            .split('T')
            .next()
            .ok_or_else(|| Error::custom("invalid date"))?;
        let mut split = date.split('-');
        let yyyy = split.next().ok_or_else(|| Error::custom("invalid date"))?;
        let mm = split.next().ok_or_else(|| Error::custom("invalid date"))?;
        let dd = split.next().ok_or_else(|| Error::custom("invalid date"))?;
        format!("{yyyy}{mm}{dd}").parse().map_err(Error::custom)
    }
}

pub(crate) fn deserialize_date<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(DateVisitor)
}

#[derive(Debug, Deserialize)]
struct Licensee {
    #[serde(alias = "userFirstname")]
    first_name: String,
    #[serde(alias = "userLastname")]
    last_name: String,
    #[serde(alias = "userEmail")]
    email: String,
    #[serde(alias = "userBirthdate", deserialize_with = "deserialize_date")]
    dob: u32,
    #[serde(alias = "userId")]
    user_id: String,
    #[serde(alias = "userLicenceNumber")]
    license_number: u32,
    #[serde(alias = "productSlug", deserialize_with = "deserialize_license_type")]
    license_type: LicenseType,
    #[serde(
        rename = "licenceStatus",
        deserialize_with = "deserialize_medical_certificate_status"
    )]
    medical_certificate_status: MedicalCertificateStatus,
}

async fn licensees() -> Option<Vec<Licensee>> {
    let current_season = current_season(None);
    let mut licensees = Vec::new();
    let mut ids = BTreeSet::new();
    let mut season = current_season;
    for _ in 0..5 {
        let mut url = Url::parse("https://api.core.myffme.fr/api/licences/unique").unwrap();
        url.query_pairs_mut()
            .append_pair("pagination", "true")
            .append_pair("itemsPerPage", "500")
            .append_pair("page", "1")
            .append_pair("season", &season.to_string())
            .append_pair("structure", &STRUCTURE_ID.to_string());
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
            let file_name = format!(".licenses_{season}.json");
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
            serde_json::from_str::<Vec<Licensee>>(&text)
                .map_err(|e| {
                    eprintln!("{e:?}");
                    e
                })
                .ok()?
        };
        #[cfg(not(test))]
        let list = response
            .json::<Vec<Licensee>>()
            .await
            .map_err(|err| {
                tracing::warn!("{err:?}");
                err
            })
            .ok()?;
        for it in list {
            if ids.insert(it.user_id.clone()) {
                licensees.push(it);
            }
        }
        season -= 1;
    }
    Some(licensees)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_user_ids() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let licensees = licensees().await.unwrap();
        println!("{licensees:?}");
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!("{}", licensees.len());
        let result = licensees
            .iter()
            .find(|&it| it.license_number == 33109)
            .unwrap();
        assert_eq!(19770522, result.dob);
        assert_eq!("DAVID", result.last_name);
        let result = licensees
            .iter()
            .find(|&it| it.license_number == 154316)
            .unwrap();
        assert_eq!(19750826, result.dob);
        assert_eq!("GRAS", result.last_name);
    }
}

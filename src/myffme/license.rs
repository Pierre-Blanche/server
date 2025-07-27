use crate::myffme::{InsuranceLevelOption, InsuranceOptionOption, LicenseType};
use crate::order::{InsuranceLevel, InsuranceOption};
use serde::de::Error;
use serde::Deserialize;

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

struct LicenseTypeVisitor;

impl<'de> serde::de::Visitor<'de> for LicenseTypeVisitor {
    type Value = LicenseType;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing a license type")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        LicenseType::try_from(v).map_err(|err| E::custom(err))
    }
}

pub(crate) fn deserialize_license_type<'de, D>(deserializer: D) -> Result<LicenseType, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(LicenseTypeVisitor)
}

struct InsuranceLevelVisitor;

impl<'de> serde::de::Visitor<'de> for InsuranceLevelVisitor {
    type Value = InsuranceLevel;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing an insurance level")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        InsuranceLevel::try_from(v).map_err(|err| E::custom(err))
    }
}

pub(crate) fn deserialize_insurance_level<'de, D>(
    deserializer: D,
) -> Result<InsuranceLevel, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(InsuranceLevelVisitor)
}

struct InsuranceOptionVisitor;

impl<'de> serde::de::Visitor<'de> for InsuranceOptionVisitor {
    type Value = InsuranceOption;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing an insurance option")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        InsuranceOption::try_from(v).map_err(|err| E::custom(err))
    }
}

pub(crate) fn deserialize_insurance_option<'de, D>(
    deserializer: D,
) -> Result<InsuranceOption, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(InsuranceOptionVisitor)
}

enum InsuranceLevelOrOption {
    InsuranceLevel(InsuranceLevel),
    InsuranceOption(InsuranceOption),
}

struct InsuranceLevelOrOptionVisitor;

impl<'de> serde::de::Visitor<'de> for InsuranceLevelOrOptionVisitor {
    type Value = InsuranceLevelOrOption;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string representing an insurance level or option")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        InsuranceLevel::try_from(v)
            .map(InsuranceLevelOrOption::InsuranceLevel)
            .or_else(|_| InsuranceOption::try_from(v).map(InsuranceLevelOrOption::InsuranceOption))
            .map_err(|err| E::custom(err))
    }
}

fn deserialize_insurance_level_or_option<'de, D>(
    deserializer: D,
) -> Result<InsuranceLevelOrOption, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(InsuranceLevelOrOptionVisitor)
}

#[derive(Deserialize)]
struct InsuranceLevelOrOptionOption {
    id: String,
    #[serde(
        rename = "slug",
        deserialize_with = "deserialize_insurance_level_or_option"
    )]
    level_or_option: InsuranceLevelOrOption,
}

pub(crate) enum ProductOption {
    InsuranceLevel(InsuranceLevelOption),
    InsuranceOption(InsuranceOptionOption),
}

pub(crate) fn deserialize_product_option<'de, D>(deserializer: D) -> Result<ProductOption, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let InsuranceLevelOrOptionOption {
        id,
        level_or_option,
    } = <InsuranceLevelOrOptionOption>::deserialize(deserializer)?;
    Ok(match level_or_option {
        InsuranceLevelOrOption::InsuranceLevel(level) => {
            ProductOption::InsuranceLevel(InsuranceLevelOption { id, level })
        }
        InsuranceLevelOrOption::InsuranceOption(option) => {
            ProductOption::InsuranceOption(InsuranceOptionOption { id, option })
        }
    })
}

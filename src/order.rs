use crate::season::is_during_discount_period;
use crate::user::LicenseType;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use tiered_server::store::Snapshot;

pub enum Order {
    License(LicenseType, InsuranceLevel, Vec<InsuranceOption>),
}

impl Order {
    pub fn price_in_cents(&self, snapshot: &Snapshot) -> u16 {
        match self {
            Self::License(license_type, insurance_level, insurance_options) => price_in_cents(
                snapshot,
                None,
                license_type,
                insurance_level,
                insurance_options.iter(),
            ),
        }
    }
}

impl Display for Order {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::License(license_type, insurance_level, insurance_options) => {
                let license_name = match license_type {
                    LicenseType::Adult => "adulte",
                    LicenseType::Child => "jeune",
                    LicenseType::Family => "famille",
                    LicenseType::NonMemberAdult => "adulte hors club",
                    LicenseType::NonMemberChild => "jeune hors club",
                    LicenseType::NonPracticing => "non pratiquant",
                };
                let insurance_level_name = match insurance_level {
                    InsuranceLevel::RC => "RC",
                    InsuranceLevel::Base => "Base",
                    InsuranceLevel::BasePlus => "Base+",
                    InsuranceLevel::BasePlusPlus => "Base++",
                };
                if insurance_options.is_empty() {
                    write!(f, "Licence {insurance_level_name} {license_name}")
                } else {
                    let s = insurance_options
                        .iter()
                        .enumerate()
                        .flat_map(|(i, it)| {
                            let name = match it {
                                InsuranceOption::MountainBike => "option VTT",
                                InsuranceOption::Ski => "option ski de piste",
                                InsuranceOption::TrailRunning => "option trail",
                                InsuranceOption::SlacklineAndHighline => {
                                    "option slackline/highline"
                                }
                            };
                            let array = if i == 0 { ["", name] } else { [", ", name] };
                            array.into_iter()
                        })
                        .collect::<String>();
                    write!(f, "Licence {insurance_level_name} {license_name} ({s})")
                }
            }
        }
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum InsuranceLevel {
    RC,
    Base,
    BasePlus,
    BasePlusPlus,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum InsuranceOption {
    MountainBike,
    Ski,
    SlacklineAndHighline,
    TrailRunning,
}

fn price_in_cents<'a>(
    snapshot: &'a Snapshot,
    timestamp: Option<u32>,
    license_type: &'a LicenseType,
    insurance_level: &'a InsuranceLevel,
    insurance_options: impl Iterator<Item = &'a InsuranceOption>,
) -> u16 {
    let key = match license_type {
        LicenseType::Adult => "cts/adult",
        LicenseType::Child => "cts/child",
        LicenseType::Family => "cts/family",
        LicenseType::NonMemberAdult => "cts/non_member_adult",
        LicenseType::NonMemberChild => "cts/non_member_child",
        LicenseType::NonPracticing => "cts/non_practicing",
    };
    let mut license_price = snapshot
        .get::<LicensePrice>(key)
        .expect("missing license price");
    if is_during_discount_period(timestamp) {
        license_price.fed_fee /= 2;
    }
    let key = match insurance_level {
        InsuranceLevel::RC => "cts/rc",
        InsuranceLevel::Base => "cts/base",
        InsuranceLevel::BasePlus => "cts/base_plus",
        InsuranceLevel::BasePlusPlus => "cts/base_plus_plus",
    };
    let insurance_level_price = snapshot.get::<u16>(key).expect("missing license price");
    let mut price = license_price.fed_fee
        + license_price.dep_fee
        + license_price.reg_fee
        + license_price.str_fee
        + insurance_level_price;
    for option in insurance_options {
        let key = match option {
            InsuranceOption::MountainBike => "cts/mountain_bike",
            InsuranceOption::Ski => "cts/ski",
            InsuranceOption::SlacklineAndHighline => "cts/slackline_and_highline",
            InsuranceOption::TrailRunning => "cts/trail_running",
        };
        let option_price = snapshot.get::<u16>(key).expect("missing license price");
        price += option_price;
    }
    price
}

#[derive(Serialize, Deserialize)]
pub struct LicensePrice {
    fed_fee: u16,
    dep_fee: u16,
    reg_fee: u16,
    str_fee: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_license_display() {
        assert_eq!(
            "Licence Base++ adulte (option VTT, option trail)",
            Order::License(
                LicenseType::Adult,
                InsuranceLevel::BasePlusPlus,
                vec![InsuranceOption::MountainBike, InsuranceOption::TrailRunning]
            )
            .to_string()
        );
        assert_eq!(
            "Licence Base jeune hors club",
            Order::License(LicenseType::NonMemberChild, InsuranceLevel::Base, vec![]).to_string()
        );
    }
}

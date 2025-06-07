use crate::season::is_during_discount_period;
use crate::user::LicenseType;
use serde::{Deserialize, Serialize};
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

pub enum InsuranceLevel {
    RC,
    Base,
    BasePlus,
    BasePlusPlus,
}

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

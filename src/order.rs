use crate::myffme::{prices, LicenseFees};
use crate::user::LicenseType;
use serde::Serialize;
use std::fmt::{Display, Formatter};
use tiered_server::store::{snapshot, Snapshot};

pub async fn update_prices() -> Option<()> {
    let snapshot = snapshot();
    let base_license_price = match snapshot.get::<u16>(BaseLicensePrice.key()) {
        Some(price) => price,
        None => {
            let price = 135_00;
            Snapshot::set_and_wait_for_update(BaseLicensePrice.key(), &price).await?;
            price
        }
    };
    if snapshot.get::<u16>(EquipmentRental.key()).is_none() {
        let price: u16 = 50_00;
        Snapshot::set_and_wait_for_update(EquipmentRental.key(), &price).await?;
    }
    let (mut license_types, mut levels, mut options) = prices(None).await?;
    for level in [
        InsuranceLevel::RC,
        InsuranceLevel::Base,
        InsuranceLevel::BasePlus,
        InsuranceLevel::BasePlusPlus,
    ] {
        let price = snapshot.get::<u16>(level.key());
        if let Some(found) = levels.remove(&level) {
            if Some(found) != price {
                Snapshot::set_and_wait_for_update(level.key(), &found).await?;
            }
        }
    }
    for option in [
        InsuranceOption::MountainBike,
        InsuranceOption::Ski,
        InsuranceOption::SlacklineAndHighline,
        InsuranceOption::TrailRunning,
    ] {
        let price = snapshot.get::<u16>(option.key());
        if let Some(found) = options.remove(&option) {
            if Some(found) != price {
                Snapshot::set_and_wait_for_update(option.key(), &found).await?;
            }
        }
    }
    let default_level_price = levels.remove(&InsuranceLevel::default())?;
    for license_type in [
        LicenseType::Adult,
        LicenseType::Child,
        LicenseType::Family,
        LicenseType::NonMemberAdult,
        LicenseType::NonMemberChild,
    ] {
        let fees = snapshot.get::<LicenseFees>(license_type.key());
        let fee = snapshot.get::<u16>(MembershipFee(license_type).key());
        if let Some(found) = license_types.remove(&license_type) {
            if Some(found.federal_fee_in_cents) != fees.as_ref().map(|it| it.federal_fee_in_cents)
                || Some(found.regional_fee_in_cents)
                    != fees.as_ref().map(|it| it.regional_fee_in_cents)
                || Some(found.department_fee_in_cents)
                    != fees.as_ref().map(|it| it.department_fee_in_cents)
            {
                Snapshot::set_and_wait_for_update(
                    license_type.key(),
                    &LicenseFees {
                        federal_fee_in_cents: found.federal_fee_in_cents,
                        regional_fee_in_cents: found.regional_fee_in_cents,
                        department_fee_in_cents: found.department_fee_in_cents,
                    },
                )
                .await?;
            }
            let expected_fee = base_license_price
                - found.federal_fee_in_cents
                - found.regional_fee_in_cents
                - found.department_fee_in_cents
                - default_level_price;
            if fee != Some(expected_fee) {
                Snapshot::set_and_wait_for_update(MembershipFee(license_type).key(), &expected_fee)
                    .await?;
            }
        }
    }
    Some(())
}

pub enum Order {
    License(
        LicenseType,
        InsuranceLevel,
        Vec<InsuranceOption>,
        Option<EquipmentRental>,
    ),
}

pub trait Priced {
    fn price_in_cents(&self, snapshot: &Snapshot, during_discount_period: bool) -> u16;
}

pub(crate) trait Keyed {
    fn key(&self) -> &'static str;
}

impl Keyed for MembershipFee {
    fn key(&self) -> &'static str {
        match self.0 {
            LicenseType::Adult => "cts/adult_structure_fee",
            LicenseType::Child => "cts/child_structure_fee",
            LicenseType::Family => "cts/family_structure_fee",
            LicenseType::NonMemberAdult => "cts/non_member_adult_structure_fee",
            LicenseType::NonMemberChild => "cts/non_member_child_structure_fee",
            LicenseType::NonPracticing => "cts/non_practicing_structure_fee",
        }
    }
}

impl Keyed for BaseLicensePrice {
    fn key(&self) -> &'static str {
        "cts/member"
    }
}

impl Keyed for LicenseType {
    fn key(&self) -> &'static str {
        match self {
            LicenseType::Adult => "cts/adult",
            LicenseType::Child => "cts/child",
            LicenseType::Family => "cts/family",
            LicenseType::NonMemberAdult => "cts/non_member_adult",
            LicenseType::NonMemberChild => "cts/non_member_child",
            LicenseType::NonPracticing => "cts/non_practicing",
        }
    }
}

impl Keyed for InsuranceLevel {
    fn key(&self) -> &'static str {
        match self {
            InsuranceLevel::RC => "cts/rc",
            InsuranceLevel::Base => "cts/base",
            InsuranceLevel::BasePlus => "cts/base_plus",
            InsuranceLevel::BasePlusPlus => "cts/base_plus_plus",
        }
    }
}

impl Keyed for InsuranceOption {
    fn key(&self) -> &'static str {
        match self {
            InsuranceOption::MountainBike => "cts/mountain_bike",
            InsuranceOption::Ski => "cts/ski",
            InsuranceOption::SlacklineAndHighline => "cts/slackline_and_highline",
            InsuranceOption::TrailRunning => "cts/trail_running",
        }
    }
}

impl Keyed for EquipmentRental {
    fn key(&self) -> &'static str {
        "cts/equipment_rental"
    }
}

impl Display for Order {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::License(license_type, insurance_level, insurance_options, equipment_rental) => {
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
                    if equipment_rental.is_none() {
                        write!(f, "Licence {insurance_level_name} {license_name}")
                    } else {
                        write!(
                            f,
                            "Licence {insurance_level_name} {license_name} (option location matériel)"
                        )
                    }
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
                    if equipment_rental.is_some() {
                        write!(
                            f,
                            "Licence {insurance_level_name} {license_name} (option location matériel, {s})"
                        )
                    } else {
                        write!(f, "Licence {insurance_level_name} {license_name} ({s})")
                    }
                }
            }
        }
    }
}

pub struct BaseLicensePrice;

pub struct MembershipFee(LicenseType);

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Serialize)]
pub enum InsuranceLevel {
    RC,
    Base,
    BasePlus,
    BasePlusPlus,
}

impl Default for InsuranceLevel {
    fn default() -> Self {
        Self::Base
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Serialize)]
pub enum InsuranceOption {
    MountainBike,
    Ski,
    SlacklineAndHighline,
    TrailRunning,
}

pub struct EquipmentRental;

impl Priced for LicenseType {
    fn price_in_cents(&self, snapshot: &Snapshot, is_during_discount_period: bool) -> u16 {
        let mut license_fees = snapshot
            .get::<LicenseFees>(self.key())
            .expect("missing license price");
        if is_during_discount_period {
            match self {
                LicenseType::Adult
                | LicenseType::Child
                | LicenseType::Family
                | LicenseType::NonPracticing => {
                    license_fees.federal_fee_in_cents /= 2;
                }
                _ => {}
            }
        }
        let structure_fee = snapshot
            .get::<u16>(MembershipFee(*self).key())
            .expect("missing structure fee");
        let federal_fee_in_cents = if is_during_discount_period {
            match self {
                LicenseType::Adult
                | LicenseType::Child
                | LicenseType::Family
                | LicenseType::NonPracticing => license_fees.federal_fee_in_cents / 2,
                _ => license_fees.federal_fee_in_cents,
            }
        } else {
            license_fees.federal_fee_in_cents
        };
        federal_fee_in_cents
            + license_fees.regional_fee_in_cents
            + license_fees.department_fee_in_cents
            + structure_fee
    }
}

impl Priced for InsuranceLevel {
    fn price_in_cents(&self, snapshot: &Snapshot, _: bool) -> u16 {
        snapshot
            .get::<u16>(self.key())
            .expect("missing insurance level price")
    }
}

impl Priced for InsuranceOption {
    fn price_in_cents(&self, snapshot: &Snapshot, _: bool) -> u16 {
        snapshot
            .get::<u16>(self.key())
            .expect("missing insurance option price")
    }
}

impl Priced for EquipmentRental {
    fn price_in_cents(&self, snapshot: &Snapshot, _: bool) -> u16 {
        snapshot
            .get::<u16>(self.key())
            .expect("missing equipment rental price")
    }
}

impl Priced for Order {
    fn price_in_cents(&self, snapshot: &Snapshot, during_discount_period: bool) -> u16 {
        match self {
            Order::License(license_type, insurance_level, insurance_options, equipment_rental) => {
                license_type.price_in_cents(snapshot, during_discount_period)
                    + insurance_level.price_in_cents(snapshot, during_discount_period)
                    + insurance_options
                        .iter()
                        .map(|it| it.price_in_cents(snapshot, during_discount_period))
                        .sum::<u16>()
                    + equipment_rental
                        .as_ref()
                        .map(|it| it.price_in_cents(snapshot, during_discount_period))
                        .unwrap_or_default()
            }
        }
    }
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
                vec![InsuranceOption::MountainBike, InsuranceOption::TrailRunning],
                None
            )
            .to_string()
        );
        assert_eq!(
            "Licence Base++ adulte (option location matériel, option VTT, option trail)",
            Order::License(
                LicenseType::Adult,
                InsuranceLevel::BasePlusPlus,
                vec![InsuranceOption::MountainBike, InsuranceOption::TrailRunning],
                Some(EquipmentRental)
            )
            .to_string()
        );
        assert_eq!(
            "Licence Base jeune hors club",
            Order::License(
                LicenseType::NonMemberChild,
                InsuranceLevel::Base,
                vec![],
                None
            )
            .to_string(),
        );
        assert_eq!(
            "Licence Base jeune hors club (option location matériel)",
            Order::License(
                LicenseType::NonMemberChild,
                InsuranceLevel::Base,
                vec![],
                Some(EquipmentRental)
            )
            .to_string(),
        );
    }
}

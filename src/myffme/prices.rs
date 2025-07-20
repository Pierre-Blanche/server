use crate::myffme::{LicenseFees, LicenseType};
use crate::order::{InsuranceLevel, InsuranceOption};
use std::collections::BTreeMap;

pub(crate) async fn prices(
    _season: Option<u16>,
) -> Option<(
    BTreeMap<LicenseType, LicenseFees>,
    BTreeMap<InsuranceLevel, u16>,
    BTreeMap<InsuranceOption, u16>,
)> {
    todo!()
}

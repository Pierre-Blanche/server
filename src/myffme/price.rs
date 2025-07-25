use crate::myffme::structure::{structure_hierarchy_by_id, StructureHierarchy};
use crate::myffme::{LicenseFees, LicenseType, STRUCTURE_ID};
use crate::order::{InsuranceLevel, InsuranceOption};
use std::collections::BTreeMap;

pub(crate) async fn prices(
    _season: Option<u16>,
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
    todo!()
}

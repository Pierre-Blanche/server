use serde::{Deserialize, Serialize};
use tiered_server::user::IdentificationMethod;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    Spouse,
    Father,
    Mother,
    GrandParent,
    Other,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmergencyContact {
    pub relationship: Relationship,
    pub last_name: String,
    pub normalized_last_name: String,
    pub first_name: String,
    pub normalized_first_name: String,
    pub identification: Vec<IdentificationMethod>,
}

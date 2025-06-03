use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Metadata {
    pub myffme_user_id: Option<String>,
    pub license_number: Option<u32>,
    pub gender: Option<Gender>,
    pub insee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_type: Option<LicenseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medical_certificate_status: Option<MedicalCertificateStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_license_season: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_structure: Option<Structure>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Female,
    Male,
    Unspecified,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    Adult,
    Child,
    Family,
    NonMemberAdult,
    NonMemberChild,
    NonPracticing,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MedicalCertificateStatus {
    Recreational,
    Competition,
    HealthQuestionnaire,
    WaitingForDocument,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Structure {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub department: Option<String>,
}

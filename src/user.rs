use crate::emergency_contact::EmergencyContact;
use crate::myffme::{CompetitionResult, Gender, LicenseType, MedicalCertificateStatus, Structure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub myffme_user_id: Option<String>,
    pub license_number: Option<u32>,
    pub gender: Option<Gender>,
    pub insee: Option<String>,
    pub city: Option<String>,
    pub zip_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_type: Option<LicenseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medical_certificate_status: Option<MedicalCertificateStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_license_season: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_structure: Option<Structure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competition_results: Option<Vec<CompetitionResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emergency_contacts: Option<Vec<EmergencyContact>>,
}

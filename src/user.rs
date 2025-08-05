use crate::emergency_contact::EmergencyContact;
use crate::myffme::address::Address;
use crate::myffme::{CompetitionResult, Gender, LicenseType, MedicalCertificateStatus, Structure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub myffme_user_id: Option<String>,
    pub license_number: Option<u32>,
    pub gender: Option<Gender>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
}

#[cfg(test)]
mod tests {
    use tiered_server::store::snapshot;
    use tiered_server::user::User;
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    #[ignore]
    async fn json() {
        let users = snapshot()
            .list::<User>("acc/")
            .map(|(_, it)| it)
            .collect::<Vec<_>>();
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(".users.json")
            .await
            .unwrap()
            .write_all(serde_json::to_string_pretty(&users).unwrap().as_bytes())
            .await
            .unwrap();
    }
}

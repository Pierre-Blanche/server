use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Metadata {
    pub license_number: Option<u32>,
    pub myffme_user_id: Option<String>,
    pub latest_license_season: Option<u16>,
    pub insee: Option<String>,
}

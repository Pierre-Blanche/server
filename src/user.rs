use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Metadata {
    pub license_number: Option<u32>,
}

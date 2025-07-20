use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
pub(crate) struct Address {
    #[serde(skip_serializing)]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

pub(crate) async fn user_address(_id: &str) -> Option<Address> {
    todo!()
}

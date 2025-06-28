use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Document {
    pub user_id: Option<String>,
    pub season: u16,
    pub category: u8,
}

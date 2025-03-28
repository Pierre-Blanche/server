use crate::env::ConfigurationKey::AdminUsers;
use crate::env::secret_value;
use crate::otp::Otp;
use crate::store::Snapshot;
use base64_simd::URL_SAFE_NO_PAD;
use pinboard::NonEmptyPinboard;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;
use zip_static_handler::handler::Handler;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum IdentificationMethod {
    Email(String),
    Sms(String),
    NotSet,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct User {
    pub(crate) id: String,
    pub(crate) identification: IdentificationMethod,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) date_of_birth: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub(crate) admin: bool,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

pub(crate) async fn ensure_admin_users_exist(
    store_cache: Arc<NonEmptyPinboard<Snapshot>>,
    handler: Arc<Handler>,
) -> Option<()> {
    let value = secret_value(AdminUsers).unwrap_or("");
    for user in value.split(";") {
        let mut iter = user.split(",");
        let email = iter.next()?;
        let first_name = iter.next()?;
        let last_name = iter.next()?;
        let date_of_birth = iter.next()?.parse::<u32>().ok()?;
        if let Some(user) = User::create(
            email.to_string(),
            first_name.to_string(),
            last_name.to_string(),
            date_of_birth,
            true,
            store_cache.clone(),
        )
        .await
        {
            Otp::send(&user, store_cache.clone(), handler.clone()).await?;
        }
    }
    Some(())
}

impl User {
    pub(crate) async fn create(
        email: String,
        first_name: String,
        last_name: String,
        date_of_birth: u32, // yyyyMMdd
        admin: bool,
        store_cache: Arc<NonEmptyPinboard<Snapshot>>,
    ) -> Option<Self> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        let mut random = [0u8; 36];
        random[32..].copy_from_slice(timestamp.to_be_bytes().as_slice());
        SystemRandom::new().fill(&mut random[..32]).unwrap();
        let id = URL_SAFE_NO_PAD.encode_to_string(
            timestamp
                .to_le_bytes()
                .into_iter()
                .chain(random.into_iter())
                .collect::<Vec<_>>(),
        );
        let identification = IdentificationMethod::Email(email);
        let key = format!("/pk/{id}");
        if store_cache.get_ref().get::<User>(key.as_str()).is_some() {
            return None;
        }
        let user = Self {
            id,
            identification,
            first_name,
            last_name,
            date_of_birth,
            admin,
        };
        Snapshot::set(key.as_str(), &user).await?;
        Some(user)
    }
}

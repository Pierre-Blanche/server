pub mod results;

use crate::mycompet::results::competition_results;
use crate::season::current_season;
use crate::user::{MedicalCertificateStatus, Metadata};
use std::sync::Arc;
use tiered_server::store::Snapshot;
use tiered_server::user::User;

pub async fn update_competition_results(snapshot: &Arc<Snapshot>) -> Option<()> {
    let season = current_season(None);
    let current_data = snapshot
        .list::<User>("acc/")
        .filter_map(|(key, mut user)| {
            if let Some(metadata) = user.metadata {
                let metadata = serde_json::from_value::<Metadata>(metadata).ok()?;
                if metadata.latest_license_season == Some(season)
                    && metadata.medical_certificate_status
                        == Some(MedicalCertificateStatus::Competition)
                    && metadata.license_number.is_some()
                {
                    user.metadata = Some(serde_json::to_value(&metadata).unwrap());
                    Some((key, (user, metadata)))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (key, (mut user, mut metadata)) in current_data.into_iter() {
        let license_number = metadata.license_number.unwrap();
        if let Some(results) = competition_results(license_number).await {
            if !results.is_empty() {
                if let Some(competition_results) = metadata.competition_results {
                    if results.len() != competition_results.len() {
                        metadata.competition_results = Some(results);
                        user.metadata = Some(serde_json::to_value(metadata).unwrap());
                        Snapshot::set_and_wait_for_update(key, &user).await?;
                    }
                }
            }
        }
    }
    Some(())
}

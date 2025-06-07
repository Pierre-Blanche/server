use tiered_server::store::snapshot;
use tiered_server::user::User;

#[tokio::main]
async fn main() {}

#[allow(dead_code)]
async fn user_entries() -> Vec<(String, User)> {
    let snapshot = snapshot(None).await.expect("failed to get store content");
    snapshot
        .list::<User>("acc/")
        .map(|(k, v)| (k.to_string(), v))
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pierre_blanche_server::myffme::{members_by_ids, update_myffme_bearer_token};
    use pierre_blanche_server::user::Metadata;
    use std::collections::BTreeMap;
    use tiered_server::store::Snapshot;

    #[tokio::test]
    async fn update_metadata() {
        let entries = user_entries().await;
        let user_ids = entries
            .iter()
            .filter_map(|(_, User { metadata, .. })| {
                metadata.as_ref().and_then(|it| {
                    serde_json::from_value::<Metadata>(it.clone())
                        .ok()
                        .and_then(|it| it.myffme_user_id.map(|it| it.to_string()))
                })
            })
            .collect::<Vec<_>>();
        if user_ids.is_empty() {
            return;
        }
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
        let members = members_by_ids(&user_ids.iter().map(|it| it.as_str()).collect::<Vec<_>>())
            .await
            .expect("failed to get members");
        let mut members_metadata = BTreeMap::new();
        for member in members {
            let metadata = member.metadata;
            if let Some(ffme_user_id) = metadata.myffme_user_id.as_ref().map(|it| it.clone()) {
                members_metadata.insert(ffme_user_id, metadata);
            }
        }
        let mut updates = Vec::new();
        for (key, mut user) in entries {
            if let Some(metadata) = user.metadata {
                let mut metadata =
                    serde_json::from_value::<Metadata>(metadata).expect("failed to parse metadata");
                if let Some(ffme_user_id) = metadata.myffme_user_id.as_ref() {
                    if let Some(found) = members_metadata.remove(ffme_user_id) {
                        let mut changed = false;
                        if metadata.insee != found.insee {
                            metadata.insee = found.insee;
                            changed = true;
                        }
                        // if metadata.zip_code != found.zip_code {
                        //     metadata.zip_code = found.zip_code;
                        //     changed = true;
                        // }
                        // if metadata.city != found.city {
                        //     metadata.city = found.city;
                        //     changed = true;
                        // }
                        if metadata.latest_license_season != found.latest_license_season {
                            metadata.latest_license_season = found.latest_license_season;
                            changed = true;
                        }
                        if metadata.medical_certificate_status.as_ref()
                            != found.medical_certificate_status.as_ref()
                        {
                            metadata.medical_certificate_status = found.medical_certificate_status;
                            changed = true;
                        }
                        if metadata.latest_structure.as_ref().map(|it| it.id)
                            != found.latest_structure.as_ref().map(|it| it.id)
                        {
                            metadata.latest_structure = found.latest_structure;
                        }
                        if changed {
                            let metadata = serde_json::to_value(&metadata)
                                .expect("failed to serialize metadata");
                            user.metadata = Some(metadata);
                            updates.push((key, user));
                        }
                    }
                }
            }
        }
        for (key, user) in updates {
            Snapshot::set(&key, &user)
                .await
                .expect("failed to update user");
        }
    }
}

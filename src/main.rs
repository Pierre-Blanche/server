use pierre_blanche_server::api::ApiExtension;
use pierre_blanche_server::update::update_loop;
use tiered_server::server::serve;

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(true)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .without_time()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "pierre_blanche_server=info,tiered_server=debug,zip_static_handler=info,hyper=info",
        ))
        .init();
    #[cfg(not(debug_assertions))]
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(true)
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .without_time()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "pierre_blanche_server=debug,tiered_server=info,zip_static_handler=info,hyper=info",
        ))
        .init();
    update_loop().await;
    serve(Box::leak(Box::new(ApiExtension))).await;
}

#[cfg(test)]
mod tests {
    use pierre_blanche_server::mycompet::competition_results;
    use pierre_blanche_server::myffme::{
        members_by_ids, members_by_structure, update_myffme_bearer_token, Member,
    };
    use pierre_blanche_server::user::Metadata;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use tiered_server::norm::{normalize_first_name, normalize_last_name};
    use tiered_server::store::{snapshot, Snapshot};
    use tiered_server::user::{Email, IdentificationMethod, User};
    use tokio::fs::File;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tracing::info;

    #[tokio::test]
    #[ignore]
    async fn test_backup() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .without_time()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "pierre_blanche_server=debug,tiered_server=debug",
            ))
            .init();
        let snapshot = snapshot();
        let backup = snapshot.backup().await.expect("failed to create backup");
        File::options()
            .write(true)
            .truncate(true)
            .create(true)
            .open("backup.tar")
            .await
            .expect("failed to open backup file")
            .write_all(&backup)
            .await
            .expect("failed to write backup file");
    }

    #[tokio::test]
    #[ignore]
    async fn test_restore() {
        let snapshot = snapshot();
        let mut backup = Vec::new();
        let _ = File::options()
            .read(true)
            .create(false)
            .write(false)
            .open("backup.tar")
            .await
            .expect("failed to open backup file")
            .read_to_end(&mut backup)
            .await
            .expect("failed to read backup file");
        snapshot
            .restore(backup.as_slice())
            .await
            .expect("failed to restore backup");
    }

    #[tokio::test]
    #[ignore]
    async fn test_add_missing_users() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .without_time()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
            ))
            .init();
        let snapshot = snapshot();
        let existing_users = snapshot
            .list::<User>("acc/")
            .map(|(_, it)| it)
            .collect::<Vec<_>>();
        info!("existing users: {}", existing_users.len());
        let existing_users_metadata = existing_users
            .iter()
            .flat_map(|it| {
                it.metadata
                    .as_ref()
                    .and_then(|value| Metadata::deserialize(value).ok())
            })
            .collect::<Vec<_>>();
        let lookup = existing_users_metadata
            .iter()
            .map(|it| (it.myffme_user_id.as_ref().unwrap(), it))
            .collect::<BTreeMap<_, _>>();
        update_myffme_bearer_token(0)
            .await
            .expect("failed to get bearer token");
        let members = members_by_structure(10, None)
            .await
            .expect("failed to get members");
        info!("members: {}", members.len());
        for member in members {
            if lookup.contains_key(member.metadata.myffme_user_id.as_ref().unwrap()) {
                continue;
            }
            let Member {
                first_name,
                last_name,
                email,
                dob,
                metadata,
                ..
            } = member;
            let identification = IdentificationMethod::Email(Email::from(email));
            let first_name_norm = normalize_first_name(first_name.as_str());
            let last_name_norm = normalize_last_name(last_name.as_str());
            if let Some(user) = existing_users
                .iter()
                .filter(|&it| {
                    it.date_of_birth == dob
                        && it.first_name_norm == first_name_norm
                        && it.last_name_norm == last_name_norm
                })
                .enumerate()
                .last()
                .and_then(|(i, it)| {
                    if i == 0 {
                        if let Some(ref myffme_user_id) = it.metadata.as_ref().and_then(|value| {
                            Metadata::deserialize(value)
                                .ok()
                                .and_then(|it| it.myffme_user_id)
                        }) {
                            panic!(
                                "user already exists {first_name} {last_name}: {myffme_user_id}"
                            );
                        } else {
                            Some(it)
                        }
                    } else {
                        panic!("multiple users found for {first_name} {last_name}");
                    }
                })
            {
                info!("updating {first_name} {last_name}");
                let metadata =
                    serde_json::to_value(&metadata).expect("failed to serialize metadata");
                let mut user = user.clone();
                user.metadata = Some(metadata);
                let key = format!("acc/{}", user.id);
                Snapshot::set_and_return_before_update(key.as_str(), &user)
                    .await
                    .expect("failed to update user");
            } else {
                let id = User::new_id(0);
                let key = format!("acc/{id}");
                info!("adding {first_name} {last_name}");
                let user = User {
                    id,
                    identification,
                    last_name,
                    last_name_norm,
                    first_name,
                    first_name_norm,
                    date_of_birth: dob,
                    admin: false,
                    metadata: Some(
                        serde_json::to_value(metadata).expect("failed to serialize metadata"),
                    ),
                };
                Snapshot::set_and_return_before_update(key.as_str(), &user)
                    .await
                    .expect("failed to add user");
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_update_existing_users_metadata() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .without_time()
            .with_env_filter(tracing_subscriber::EnvFilter::new(
                "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
            ))
            .init();
        let snapshot = snapshot();
        let entries = snapshot
            .list::<User>("acc/")
            .map(|(k, v)| (k.to_string(), v))
            .collect::<Vec<_>>();
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
        info!("existing users: {}", entries.len());
        update_myffme_bearer_token(0)
            .await
            .expect("failed to get bearer token");
        let members = members_by_ids(
            &user_ids.iter().map(|it| it.as_str()).collect::<Vec<_>>(),
            None,
        )
        .await
        .expect("failed to get members");
        info!("members: {}", members.len());
        let mut members_metadata = BTreeMap::new();
        let mut members_competition_results = BTreeMap::new();
        for member in members {
            let metadata = member.metadata;
            if let Some(ffme_user_id) = metadata.myffme_user_id.as_ref().map(|it| it.clone()) {
                if let Some(license_number) = metadata.license_number {
                    if let Some(competition_results) = competition_results(license_number).await {
                        members_competition_results
                            .insert(ffme_user_id.clone(), competition_results);
                    }
                }
                members_metadata.insert(ffme_user_id, metadata);
            }
        }
        info!("competition results: {}", members_competition_results.len());
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
                            changed = true;
                        }
                        let competition_results = members_competition_results.remove(ffme_user_id);
                        if let Some(competition_results) = competition_results {
                            if metadata
                                .competition_results
                                .as_ref()
                                .map(|it| it.len())
                                .unwrap_or(0)
                                != competition_results.len()
                            {
                                metadata.competition_results = Some(competition_results);
                                changed = true;
                            }
                        }
                        if changed {
                            info!("updating {} {}", user.first_name, user.last_name);
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
            Snapshot::set_and_return_before_update(&key, &user)
                .await
                .expect("failed to update user");
        }
    }
}

use pierre_blanche_server::myffme::{search, update_bearer_token};
use pierre_blanche_server::user::Metadata;
use tiered_server::norm::{normalize_first_name, normalize_last_name};
use tiered_server::store::{snapshot, Snapshot};
use tiered_server::user::User;

#[tokio::main]
async fn main() {
    let snapshot = snapshot(None).await.expect("failed to get store content");
    assert!(update_bearer_token(0).await.is_some());
    let mut updates = Vec::new();
    for (key, mut user) in snapshot.list::<User>("acc/") {
        let mut metadata = user
            .metadata
            .map(|it| serde_json::from_value(it).expect("failed to deserialize metadata"))
            .unwrap_or(Metadata::default());
        let results = search(
            Some(&format!("{} {}", user.first_name, user.last_name)),
            Some(user.date_of_birth),
            metadata.license_number,
        )
        .await
        .expect(&format!(
            "failed to search for user: {} {}",
            user.first_name, user.last_name
        ));
        let normalized_last_name = normalize_last_name(&user.last_name);
        let normalized_first_name = normalize_last_name(&user.first_name);
        let mut iter = results.into_iter().filter(|it| {
            normalize_last_name(&it.licensee.last_name) == normalized_last_name
                && normalize_first_name(&it.licensee.first_name) == normalized_first_name
                && it.licensee.dob == user.date_of_birth
        });
        let first = iter.next().expect(&format!(
            "failed to find user: {} {}",
            user.first_name, user.last_name
        ));
        assert!(
            iter.next().is_none(),
            "found more than one user for {} {}",
            user.first_name,
            user.last_name
        );
        if metadata.license_number != Some(first.licensee.license_number)
            || metadata.latest_license_season != first.latest_license_season
            || metadata.myffme_user_id.as_deref() != Some(&first.licensee.id)
        {
            metadata.license_number = Some(first.licensee.license_number);
            metadata.latest_license_season = first.latest_license_season;
            metadata.myffme_user_id = Some(first.licensee.id);
            user.metadata =
                Some(serde_json::to_value(metadata).expect("failed to serialize metadata"));
            updates.push((key, user));
        }
    }
    for (key, user) in updates {
        Snapshot::set(key, &user).await.expect(&format!(
            "failed to update user: {} {}",
            user.first_name, user.last_name
        ));
        println!("updated user: {} {}", user.first_name, user.last_name);
    }
}

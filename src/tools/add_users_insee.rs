use pierre_blanche_server::address::cities_by_zip_code;
use pierre_blanche_server::myffme::{update_address, update_bearer_token, user_addresses};
use pierre_blanche_server::user::Metadata;
use tiered_server::norm::normalize_city;
use tiered_server::store::{snapshot, Snapshot};
use tiered_server::user::User;

#[tokio::main]
async fn main() {
    let snapshot = snapshot(None).await.expect("failed to get store content");
    println!(
        "{}",
        update_bearer_token(0)
            .await
            .expect("failed to get bearer token")
    );
    let mut updates = Vec::new();
    let users = snapshot.list::<User>("acc/").collect::<Vec<_>>();
    let ids = users
        .iter()
        .map(|(_key, user)| user.id.as_str())
        .collect::<Vec<_>>();
    println!("{} users", users.len());
    let mut addresses = user_addresses(&ids)
        .await
        .expect("failed to get user addresses");
    for (key, mut user) in users {
        let mut metadata = user
            .metadata
            .map(|it| serde_json::from_value(it).expect("failed to deserialize metadata"))
            .unwrap_or(Metadata::default());
        let mut address = addresses.remove(user.id.as_str()).expect(&format!(
            "failed to get address for user: {} {}",
            user.first_name, user.last_name
        ));
        if address.insee.is_none() {
            if let Some(city_name) = address.city {
                let zip_code = address.zip_code.expect(&format!(
                    "missing zip code for user: {}, {}",
                    user.first_name, user.last_name
                ));
                let normalized_city_name = normalize_city(&city_name);
                match cities_by_zip_code(&zip_code)
                    .await
                    .expect("failed to search for city")
                    .into_iter()
                    .find(|it| normalize_city(&it.name) == normalized_city_name)
                {
                    Some(city) => {
                        update_address(&user.id, &zip_code, &city)
                            .await
                            .expect("failed to update address");
                        address.insee = Some(city.insee);
                    }
                    _ => {
                        eprintln!(
                            "missing insee for user: {} {}",
                            user.first_name, user.last_name
                        );
                    }
                }
            } else {
                eprintln!(
                    "missing city and insee for user: {} {}",
                    user.first_name, user.last_name
                );
            }
        }
        if metadata.insee != address.insee {
            metadata.insee = address.insee;
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

use pierre_blanche_server::address::cities_by_zip_code;
use pierre_blanche_server::myffme::{
    member_by_license_number, update_address, update_bearer_token,
};
use tiered_server::norm::normalize_city;

#[tokio::main]
async fn main() {}

async fn update_address_for_user_by_license_number(
    license_number: u32,
    city_name: &str,
    zip_code: &str,
) -> Option<()> {
    println!(
        "{}",
        update_bearer_token(0)
            .await
            .expect("failed to get bearer token")
    );
    let user = member_by_license_number(license_number)
        .await
        .expect("failed to search for user");
    let normalized_city_name = normalize_city(city_name);
    let user_id = user.metadata.myffme_user_id.expect("missing user id");
    println!("user id: {user_id}");
    let city = cities_by_zip_code(zip_code)
        .await
        .expect("failed to search for city")
        .into_iter()
        .find(|it| normalize_city(&it.name) == normalized_city_name)
        .expect("failed to find city");
    update_address(&user_id, zip_code, &city).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pierre_blanche_server::address::alternate_city_names;
    use pierre_blanche_server::user::Metadata;
    use serde::Deserialize;
    use tokio::fs::File;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_update() {
        let license_number = 991297_u32;
        let zip_code = "85200";
        let city_name = "Fontenay-le-Comte";
        assert!(
            update_address_for_user_by_license_number(license_number, city_name, zip_code)
                .await
                .is_some()
        );
    }

    #[derive(Deserialize)]
    struct User {
        first_name: String,
        last_name: String,
        metadata: Metadata,
    }

    #[tokio::test]
    async fn test_count_mismatches() {
        let mut content = String::new();
        &mut File::open(".list.json")
            .await
            .expect("missing user list file")
            .read_to_string(&mut content)
            .await
            .expect("failed to read user list file");
        let members = serde_json::from_str::<Vec<User>>(content.as_str())
            .expect("failed to parse user list file");
        let mut count = 0_usize;
        for member in members.iter() {
            let metadata = &member.metadata;
            if metadata.insee.is_some() {
                continue;
            }
            if match metadata {
                Metadata {
                    zip_code: Some(zip_code),
                    city: Some(city_name),
                    ..
                } => {
                    let normalized_city_name = normalize_city(city_name);
                    let cities = cities_by_zip_code(zip_code)
                        .await
                        .expect("failed to search for city");
                    let mut result = cities
                        .iter()
                        .find(|&it| normalize_city(&it.name) == normalized_city_name);
                    if result.is_none() {
                        'city: for city in cities.iter() {
                            if let Some(alternate_names) = alternate_city_names(&city.insee).await {
                                if alternate_names
                                    .into_iter()
                                    .any(|it| normalize_city(&it) == normalized_city_name)
                                {
                                    result = Some(city);
                                    break 'city;
                                }
                            }
                        }
                    }
                    if result.is_none() {
                        count += 1;
                        true
                    } else {
                        //println!("{}", result.unwrap().name);
                        false
                    }
                }
                _ => {
                    count += 1;
                    true
                }
            } {
                eprintln!(
                    "mismatch for user {} {} with id {} and license #{}; city: {} {}",
                    member.first_name.as_str(),
                    member.last_name.as_str(),
                    metadata
                        .myffme_user_id
                        .as_ref()
                        .map(|it| it.as_str())
                        .unwrap_or("?"),
                    metadata.license_number.as_ref().copied().unwrap_or(0),
                    metadata.city.as_ref().map(|it| it.as_str()).unwrap_or("?"),
                    metadata
                        .zip_code
                        .as_ref()
                        .map(|it| it.as_str())
                        .unwrap_or("?")
                );
            }
        }
        println!("{} users", members.len());
    }
}

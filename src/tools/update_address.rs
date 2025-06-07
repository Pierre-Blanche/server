use pierre_blanche_server::address::cities_by_zip_code;
use pierre_blanche_server::myffme::{
    member_by_license_number, update_address, update_myffme_bearer_token,
};
use tiered_server::norm::normalize_city;

#[tokio::main]
async fn main() {}

#[allow(dead_code)]
async fn update_address_for_user_by_license_number(
    license_number: u32,
    city_name: &str,
    zip_code: &str,
    line1: Option<&str>,
    country_id: Option<u16>,
) -> Option<()> {
    println!(
        "{}",
        update_myffme_bearer_token(0)
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
    update_address(&user_id, zip_code, &city, line1, country_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pierre_blanche_server::address::{alternate_city_names, city_name_by_insee, City};
    use pierre_blanche_server::myffme::members_by_structure;
    use pierre_blanche_server::user::Metadata;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn test_token() {
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
    }

    #[tokio::test]
    async fn test_update() {
        let license_number = 912468_u32;
        let zip_code = "79160";
        let city_name = "St Pompain";
        let line1 = Some("9 rue du moulin à vent");
        let country_id = None;
        assert!(
            update_address_for_user_by_license_number(
                license_number,
                city_name,
                zip_code,
                line1,
                country_id
            )
            .await
            .is_some()
        );
    }

    #[tokio::test]
    async fn test_fix_city_names() {
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
        let members = members_by_structure(10)
            .await
            .expect("failed to get members");
        let mut insee_to_city_names = BTreeMap::new();
        for member in members.iter() {
            let metadata = &member.metadata;
            if metadata.insee.is_none() {
                continue;
            }
            match metadata {
                Metadata {
                    city: Some(city_name),
                    zip_code: Some(zip_code),
                    insee: Some(insee),
                    ..
                } => {
                    let name = match insee_to_city_names.get(insee) {
                        Some(it) => it,
                        None => {
                            let it = city_name_by_insee(insee).await.unwrap_or_else(|| {
                                panic!("failed to get city name for insee: {insee}")
                            });
                            let _ = insee_to_city_names.insert(insee.clone(), it);
                            insee_to_city_names.get(insee).unwrap()
                        }
                    };
                    if name != city_name {
                        if update_address(
                            &metadata.myffme_user_id.as_ref().unwrap(),
                            zip_code,
                            &City {
                                name: name.to_string(),
                                insee: insee.clone(),
                            },
                            None,
                            None,
                        )
                        .await
                        .is_some()
                        {
                            println!("updated {city_name} -> {name} {insee}");
                        } else {
                            eprintln!("failed to update {city_name} -> {name} {insee}");
                        }
                    }
                }
                _ => {}
            }
        }
        println!("{} users", members.len());
    }

    #[tokio::test]
    async fn test_add_missing_insee() {
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
        let members = members_by_structure(10)
            .await
            .expect("failed to get members");
        for member in members.iter() {
            let metadata = &member.metadata;
            if metadata.insee.is_some() {
                continue;
            }
            if let Metadata {
                zip_code: Some(zip_code),
                city: Some(city_name),
                ..
            } = metadata
            {
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
                if let Some(city) = result {
                    if update_address(
                        &metadata.myffme_user_id.as_ref().unwrap(),
                        zip_code,
                        city,
                        None,
                        None,
                    )
                    .await
                    .is_some()
                    {
                        println!("updated {city_name} -> {} {}", city.name, city.insee)
                    } else {
                        eprintln!(
                            "failed to update {city_name} -> {} {}",
                            city.name, city.insee
                        );
                        panic!("aborting");
                    }
                }
            }
        }
        println!("{} users", members.len());
    }

    #[tokio::test]
    async fn test_count_insee_mismatch() {
        println!(
            "{}",
            update_myffme_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
        let members = members_by_structure(10)
            .await
            .expect("failed to get members");
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

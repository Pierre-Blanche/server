use pierre_blanche_server::address::cities_by_zip_code;
use pierre_blanche_server::myffme::{licensee, update_address, update_bearer_token};
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
    let user = licensee(None, None, None, Some(license_number))
        .await
        .expect("failed to search for user");
    let normalized_city_name = normalize_city(city_name);
    let mut iter = user.into_iter();
    let user = iter.next().expect("failed to find user");
    assert!(iter.next().is_none(), "found more than one user");
    println!("user id: {}", user.id);
    let city = cities_by_zip_code(zip_code)
        .await
        .expect("failed to search for city")
        .into_iter()
        .find(|it| normalize_city(&it.name) == normalized_city_name)
        .expect("failed to find city");
    update_address(&user.id, zip_code, &city).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pierre_blanche_server::address::alternate_city_names;
    use pierre_blanche_server::myffme::{Address, Gender, LicenseType, MedicalCertificateStatus};
    use serde::Deserialize;
    use tokio::fs::File;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_update() {
        let license_number = 33109_u32;
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
        id: String,
        gender: Gender,
        first_name: String,
        last_name: String,
        birth_name: Option<String>,
        birth_place: Option<String>,
        dob: u32,
        email: String,
        phone_number: Option<String>,
        username: Option<String>,
        active_license: bool,
        license_type: LicenseType,
        medical_certificate_status: MedicalCertificateStatus,
        last_license_season: Option<u32>,
        address: Address,
        license_number: u32,
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
        let licensees = serde_json::from_str::<Vec<User>>(content.as_str())
            .expect("failed to parse user list file");
        let mut count = 0_usize;
        for user in licensees.iter() {
            if user.address.insee.is_some() {
                continue;
            }
            if match &user.address {
                Address {
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
                    "mismatch for user {} {} with id {} and license #{}; address: {}",
                    user.first_name,
                    user.last_name,
                    user.id,
                    user.license_number,
                    serde_json::to_string(&user.address).unwrap()
                );
            }
        }
        println!("{} users", licensees.len());
    }
}

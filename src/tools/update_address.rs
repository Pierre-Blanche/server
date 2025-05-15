use pierre_blanche_server::address::city_by_zip_code;
use pierre_blanche_server::myffme::{search, update_address, update_bearer_token};

#[tokio::main]
async fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_update() {
        println!(
            "{}",
            update_bearer_token(0)
                .await
                .expect("failed to get bearer token")
        );
        let user = search(None, None, Some(33109))
            .await
            .expect("failed to search for user");
        let mut iter = user.into_iter();
        let user = iter.next().expect("failed to find user");
        assert!(iter.next().is_none(), "found more than one user");
        println!("user id: {}", user.licensee.id);
        let city = city_by_zip_code("85200")
            .await
            .expect("failed to search for city")
            .into_iter()
            .find(|it| {
                it.name
                    .chars()
                    .map(|it| {
                        if it.is_ascii_alphabetic() {
                            it.to_ascii_lowercase()
                        } else {
                            ' '
                        }
                    })
                    .collect::<String>()
                    == "fontenay le comte"
            })
            .expect("failed to find flc");
        assert!(
            update_address(&user.licensee.id, "85200", &city)
                .await
                .is_some()
        );
    }
}

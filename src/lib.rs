pub mod address;
pub mod api;
mod category;
mod chrome;
mod hello_asso;
mod http_client;
pub mod mycompet;
pub mod myffme;
mod order;
mod season;
pub mod update;
pub mod user;

#[cfg(test)]
mod tests {
    use crate::myffme::{add_missing_users, update_myffme_bearer_token, update_users_metadata};
    use tiered_server::store::snapshot;
    use tokio::fs::File;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        update_myffme_bearer_token(0)
            .await
            .expect("failed to get bearer token");
        add_missing_users(&snapshot, None, false).await.unwrap();
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
        update_myffme_bearer_token(0)
            .await
            .expect("failed to get bearer token");
        update_users_metadata(&snapshot, false).await.unwrap();
    }
}

use pierre_blanche_server::api::ApiExtension;
use pierre_blanche_server::update::update_loop;
use tiered_server::server::serve;
use tokio_rustls::rustls::crypto::ring::default_provider as ring_default_provider;

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
            "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
        ))
        .init();
        // Crypto provider setup
    ring_default_provider()
        .install_default()
        .expect("Failed to install ring as default crypto provider");
    #[cfg(not(debug_assertions))]
    tracing_subscriber::fmt()
        .compact()
        .with_ansi(true)
        .with_target(false)
        .with_file(false)
        .with_line_number(false)
        .without_time()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "pierre_blanche_server=debug,tiered_server=debug,zip_static_handler=info,hyper=info",
        ))
        .init();
    update_loop().await;
    serve(Box::leak(Box::new(ApiExtension))).await;
}

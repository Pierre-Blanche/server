mod api;
mod auth;
mod email;
mod env;
mod firewalls;
mod headers;
mod norm;
mod otp;
mod prefix;
mod push_webhook;
mod session;
mod store;
mod user;

extern crate rustls as extern_rustls;

use crate::api::handle_api;
use crate::env::ConfigurationKey::{
    BindAddress, DomainApex, DomainTitle, StaticGithubBranch, StaticGithubRepository,
    StaticGithubUser,
};
use crate::env::secret_value;
use crate::firewalls::update_firewall_loop;
use crate::headers::HTML;
use crate::prefix::{API_PATH_PREFIX, USER_PATH_PREFIX};
use crate::push_webhook::handle_webhook;
use crate::session::{LOGIN_PATH, SID_EXPIRED, SessionState};
use crate::store::{snapshot, update_store_cache_loop};
use crate::user::ensure_admin_users_exist;
use extern_rustls::ServerConfig;
use extern_rustls::crypto::ring::sign::any_supported_type;
use extern_rustls::pki_types::PrivateKeyDer;
use extern_rustls::server::{Acceptor, ClientHello as RustClientHello, ResolvesServerCert};
use extern_rustls::sign::CertifiedKey;
use firewall::Accept;
use firewall::builder::Firewall;
use firewall::cloudflare::fetch_cloudflare_ip_ranges;
use firewall::github::fetch_github_webhook_ip_ranges;
use headers::HSelector;
use http_body_util::{Either, Empty};
use hyper::header::{HeaderValue, LOCATION, SET_COOKIE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Response, StatusCode};
use hyper_util::rt::TokioIo;
use pinboard::NonEmptyPinboard;
use rcgen::generate_simple_self_signed;
use reqwest::Client;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use tokio::net::TcpListener;
use tokio::spawn;
use tokio_rustls::LazyConfigAcceptor;
use zip_static_handler::github::zip_download_branch_url;
use zip_static_handler::handler::Handler;
use zip_static_handler::http::headers::CONTENT_TYPE;

//noinspection SpellCheckingInspection
static DOMAIN_APEX: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(DomainApex).expect("missing domain name"));

//noinspection SpellCheckingInspection
pub static DOMAIN_TITLE: LazyLock<&'static str> =
    LazyLock::new(|| secret_value(DomainTitle).expect("missing domain title"));

#[derive(Debug)]
struct LocalhostResolver {
    key: Arc<CertifiedKey>,
}

impl Default for LocalhostResolver {
    /// Create self-signed certificate for domain "localhost" and ip "127.0.0.1".
    fn default() -> Self {
        let cert = generate_simple_self_signed(vec![
            "localhost".to_string(),
            format!("{}", Ipv4Addr::LOCALHOST),
        ])
        .expect("failed to generate self-signed certificate for localhost");
        let key = Arc::new(CertifiedKey::new(
            vec![cert.cert.der().to_vec().into()],
            any_supported_type(&PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into()))
                .expect("failed to generate signing key"),
        ));
        Self { key }
    }
}

impl ResolvesServerCert for LocalhostResolver {
    fn resolve(&self, _client_hello: RustClientHello) -> Option<Arc<CertifiedKey>> {
        Some(self.key.clone())
    }
}

async fn download(url: &str) -> Result<Vec<u8>, reqwest::Error> {
    let response = Client::default().get(url).send().await?;
    if !response.status().is_success() {
        panic!("failed to download {url} ({})", response.status().as_str());
    }
    Ok(response.bytes().await?.to_vec())
}

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
            "pb-server=debug,zip_static_handler=info,hyper=info",
        ))
        .init();

    let cloudflare_ip_ranges = fetch_cloudflare_ip_ranges()
        .await
        .expect("failed to fetch cloudflare ip ranges");
    let github_ip_ranges = fetch_github_webhook_ip_ranges()
        .await
        .expect("failed to fetch github webhook ip ranges");
    let _domain_title = *DOMAIN_TITLE;
    let domain_apex = *DOMAIN_APEX;
    let domains = vec![
        domain_apex.to_string(),
        format!("www.{domain_apex}"),
        format!("{domain_apex}.localhost"),
        "localhost".to_string(),
        "www.localhost".to_string(),
    ];
    let firewall = Arc::new(NonEmptyPinboard::new(
        Firewall::default()
            .require_sni()
            .allow_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .allow_ip(IpAddr::V6(Ipv6Addr::LOCALHOST))
            .allow_ip_ranges(cloudflare_ip_ranges.iter().cloned())
            .allow_server_names(domains.iter().cloned()),
    ));
    let webhook_firewall = Arc::new(NonEmptyPinboard::new(
        Firewall::default()
            .require_sni()
            .allow_ip_ranges(github_ip_ranges.iter().cloned())
            .allow_ip_ranges(cloudflare_ip_ranges.iter().cloned())
            .allow_server_names(domains.iter().cloned()),
    ));

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(LocalhostResolver::default()));

    update_firewall_loop(
        firewall.clone(),
        webhook_firewall.clone(),
        domains.clone(),
        domains.clone(),
        cloudflare_ip_ranges.clone(),
        github_ip_ranges.clone(),
    );

    let github_user =
        secret_value(StaticGithubUser).expect("missing github user for static content repository");
    let github_repository = secret_value(StaticGithubRepository)
        .expect("missing github repository name for static content repository");
    let github_branch = secret_value(StaticGithubBranch)
        .expect("missing github repository branch for static content repository");
    let zip = download(&zip_download_branch_url(
        github_user,
        github_repository,
        github_branch,
    ))
    .await
    .expect("failed to download static content");
    let api_path_prefix = API_PATH_PREFIX.deref();
    let user_path_prefix = USER_PATH_PREFIX.deref();
    let login_path = *LOGIN_PATH.deref();
    let listener = TcpListener::bind((secret_value(BindAddress).unwrap_or("0.0.0.0"), 443u16))
        .await
        .expect("could not bind to 443");
    let static_handler = Handler::builder()
        .with_custom_header_selector(&HSelector)
        .with_zip_prefix(format!("{github_repository}-{github_branch}/"))
        .with_zip(zip)
        .try_build()
        .expect("failed to extract static content");

    let snapshot = snapshot(None).await.expect("failed to cache store content");
    let store_cache = Arc::new(NonEmptyPinboard::new(snapshot));
    let static_handler = Arc::new(static_handler);
    ensure_admin_users_exist(store_cache.clone(), static_handler.clone())
        .await
        .expect("failed to get or create admin users");
    update_store_cache_loop(store_cache.clone());

    loop {
        if let Ok((tcp_stream, remote_address)) = listener.accept().await {
            let store_cache = store_cache.clone();
            let firewall = firewall.clone();
            let webhook_firewall = webhook_firewall.clone();
            let config = config.clone();
            let static_handler = static_handler.clone();
            spawn(async move {
                let acceptor = LazyConfigAcceptor::new(Acceptor::default(), tcp_stream);
                if let Ok(start_handshake) = acceptor.await {
                    let client_hello = &start_handshake.client_hello();
                    let ip = remote_address.ip();
                    // webhook calls should all originate from GitHub servers
                    let is_webhook = if firewall.get_ref().accept(ip, Some(client_hello)) {
                        false
                    }
                    // all other requests should original from Cloudflare servers
                    else if webhook_firewall.get_ref().accept(ip, Some(client_hello)) {
                        true
                    } else {
                        return;
                    };
                    if let Ok(stream) = start_handshake.into_stream(Arc::new(config)).await {
                        let io = TokioIo::new(stream);
                        let _ = http1::Builder::new()
                            .serve_connection(
                                io,
                                service_fn(move |request| {
                                    let handler = static_handler.clone();
                                    let store_cache = store_cache.clone();
                                    async move {
                                        let path = request.uri().path();
                                        // webhook call from the GitHub repository that notifies
                                        // that the static content should be updated
                                        if is_webhook {
                                            Ok::<_, Infallible>(handle_webhook(request).await)
                                        }
                                        // api requests
                                        else if api_path_prefix.matches(path) {
                                            Ok::<_, Infallible>(
                                                handle_api(request, store_cache, handler).await,
                                            )
                                        } else {
                                            if user_path_prefix.matches(path) {
                                                // user scoped html pages that require login
                                                if let Some(HTML) =
                                                    handler.entry(path).and_then(|it| {
                                                        it.headers.iter().find_map(|it| {
                                                            if it.key == CONTENT_TYPE {
                                                                Some(it.value.as_ref())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                    })
                                                {
                                                    match SessionState::from_headers(
                                                        request.headers(),
                                                        &store_cache,
                                                    )
                                                    .await
                                                    {
                                                        SessionState::Valid { .. } => {}
                                                        _ => {
                                                            // redirect to the login page
                                                            let response = match request.method() {
                                                                &Method::HEAD | &Method::GET => {
                                                                    let mut response =
                                                                        Response::builder().status(
                                                                            StatusCode::FOUND,
                                                                        );
                                                                    let headers = response
                                                                        .headers_mut()
                                                                        .unwrap();
                                                                    headers.insert(
                                                                        LOCATION,
                                                                        HeaderValue::from_static(
                                                                            login_path,
                                                                        ),
                                                                    );
                                                                    headers.insert(
                                                                        SET_COOKIE,
                                                                        SID_EXPIRED,
                                                                    );
                                                                    response
                                                                }
                                                                _ => Response::builder()
                                                                    .status(StatusCode::FORBIDDEN),
                                                            };
                                                            return Ok::<_, Infallible>(
                                                                response
                                                                    .body(Either::Right(
                                                                        Empty::new(),
                                                                    ))
                                                                    .unwrap(),
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            // static content
                                            Ok::<_, Infallible>(
                                                handler.handle_hyper_request(request),
                                            )
                                        }
                                    }
                                }),
                            )
                            .await;
                    }
                }
            });
        }
    }
}

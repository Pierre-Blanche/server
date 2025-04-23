use http_body_util::{Either, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use std::sync::Arc;
use tiered_server::api::Extension;
use tiered_server::store::Snapshot;

#[tokio::main]
async fn main() {
    tiered_server::server::serve(Box::leak(Box::new(ApiExtension)));
}

struct ApiExtension;

impl Extension for ApiExtension {
    async fn handle_api_extension(
        &self,
        request: Request<Incoming>,
        store_cache: &Arc<pinboard::NonEmptyPinboard<Snapshot>>,
        handler: Arc<zip_static_handler::handler::Handler>,
        server_name: Arc<String>,
    ) -> Option<Response<Either<Full<Bytes>, Empty<Bytes>>>> {
        None
    }
}

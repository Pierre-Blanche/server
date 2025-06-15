use crate::category::Category;
use crate::order::{EquipmentRental, InsuranceLevel, InsuranceOption, Priced};
use crate::season::{current_season, is_during_discount_period};
use crate::user::LicenseType;
use http_body_util::{Either, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{ALLOW, CONTENT_TYPE};
use hyper::{Request, Response, StatusCode};
use serde::Serialize;
use std::sync::Arc;
use tiered_server::api::Extension;
use tiered_server::headers::{GET_POST_PUT, JSON};
use tiered_server::session::SessionState;
use tiered_server::store::snapshot;
use tiered_server::user::User;
use tracing::debug;

pub struct ApiExtension;

impl Extension for ApiExtension {
    async fn handle_api_extension(
        &self,
        request: Request<Incoming>,
        server_name: &Arc<String>,
    ) -> Option<Response<Either<Full<Bytes>, Empty<Bytes>>>> {
        let path = request.uri().path().strip_prefix("/api")?;
        if let Some(path) = path.strip_prefix("/user") {
            if let Some(path) = path.strip_prefix("/admin") {
                if path == "/users" {
                    if request.method() != hyper::Method::GET {
                        let mut response = Response::builder();
                        let headers = response.headers_mut().unwrap();
                        headers.insert(ALLOW, GET_POST_PUT);
                        debug!("405 https://{server_name}/api/user/admin/users");
                        return Some(
                            response
                                .status(StatusCode::METHOD_NOT_ALLOWED)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                    let snapshot = snapshot();
                    if SessionState::from_headers(request.headers(), &snapshot)
                        .await
                        .is_admin()
                    {
                        let users = snapshot.list::<User>("acc/").collect::<Vec<_>>();
                        debug!("200 https://{server_name}/api/user/admin/users");
                        return Some(
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(CONTENT_TYPE, JSON)
                                .body(Either::Left(Full::from(
                                    serde_json::to_vec(&users).unwrap(),
                                )))
                                .unwrap(),
                        );
                    } else {
                        debug!("403 https://{server_name}/api/user/admin/users");
                        return Some(
                            Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                } else if path == "/registrations" {
                    if request.method() != hyper::Method::GET {
                        let mut response = Response::builder();
                        let headers = response.headers_mut().unwrap();
                        headers.insert(ALLOW, GET_POST_PUT);
                        debug!("405 https://{server_name}/api/user/admin/registrations");
                        return Some(
                            response
                                .status(StatusCode::METHOD_NOT_ALLOWED)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                    let snapshot = snapshot();
                    if SessionState::from_headers(request.headers(), &snapshot)
                        .await
                        .is_admin()
                    {
                        let users = snapshot.list::<User>("reg/").collect::<Vec<_>>();
                        debug!("200 https://{server_name}/api/user/admin/registrations");
                        return Some(
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(CONTENT_TYPE, JSON)
                                .body(Either::Left(Full::from(
                                    serde_json::to_vec(&users).unwrap(),
                                )))
                                .unwrap(),
                        );
                    } else {
                        debug!("403 https://{server_name}/api/user/admin/registrations");
                        return Some(
                            Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                }
            } else if path == "/prices" {
                if request.method() != hyper::Method::GET {
                    let mut response = Response::builder();
                    let headers = response.headers_mut().unwrap();
                    headers.insert(ALLOW, GET_POST_PUT);
                    debug!("405 https://{server_name}/api/user/prices");
                    return Some(
                        response
                            .status(StatusCode::METHOD_NOT_ALLOWED)
                            .body(Either::Right(Empty::new()))
                            .unwrap(),
                    );
                }
                let snapshot = snapshot();
                if let SessionState::Valid { user, .. } =
                    SessionState::from_headers(request.headers(), &snapshot).await
                {
                    let is_during_discount_period = is_during_discount_period(None);
                    let season = current_season(None);
                    let license_type =
                        if Category::from_dob(user.date_of_birth, season) < Category::U18 {
                            LicenseType::Child
                        } else {
                            LicenseType::Adult
                        };
                    let license_price =
                        license_type.price_in_cents(&snapshot, is_during_discount_period);
                    let base_level = InsuranceLevel::Base;
                    let base_level_price =
                        base_level.price_in_cents(&snapshot, is_during_discount_period);
                    let base_price_in_cents = license_price + base_level_price;
                    let insurance_options =
                        [InsuranceLevel::BasePlus, InsuranceLevel::BasePlusPlus]
                            .into_iter()
                            .map(|level| {
                                let price_in_cents = level
                                    .price_in_cents(&snapshot, is_during_discount_period)
                                    - base_price_in_cents;
                                PricedLevel {
                                    level,
                                    price_in_cents,
                                }
                            })
                            .collect::<Vec<_>>();
                    let addons = [
                        InsuranceOption::Ski,
                        InsuranceOption::MountainBike,
                        InsuranceOption::SlacklineAndHighline,
                        InsuranceOption::TrailRunning,
                    ]
                    .into_iter()
                    .map(|option| {
                        let price_in_cents =
                            option.price_in_cents(&snapshot, is_during_discount_period);
                        PricedAddon {
                            option,
                            price_in_cents,
                        }
                    })
                    .collect::<Vec<_>>();
                    let equipment_rental_price_in_cents =
                        EquipmentRental.price_in_cents(&snapshot, is_during_discount_period);
                    debug!("200 https://{server_name}/api/user/prices");
                    return Some(
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, JSON)
                            .body(Either::Left(Full::from(
                                serde_json::to_vec(&Prices {
                                    base_price_in_cents,
                                    insurance_options,
                                    addons,
                                    equipment_rental_price_in_cents,
                                })
                                .unwrap(),
                            )))
                            .unwrap(),
                    );
                } else {
                    debug!("403 https://{server_name}/api/user/prices");
                    return Some(
                        Response::builder()
                            .status(StatusCode::FORBIDDEN)
                            .body(Either::Right(Empty::new()))
                            .unwrap(),
                    );
                }
            }
        }
        None
    }
}

#[derive(Serialize)]
struct Prices {
    base_price_in_cents: u16,
    insurance_options: Vec<PricedLevel>,
    addons: Vec<PricedAddon>,
    equipment_rental_price_in_cents: u16,
}

#[derive(Serialize)]
struct PricedLevel {
    level: InsuranceLevel,
    price_in_cents: u16,
}

#[derive(Serialize)]
struct PricedAddon {
    option: InsuranceOption,
    price_in_cents: u16,
}

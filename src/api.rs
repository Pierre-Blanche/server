use crate::category::Category;
use crate::myffme::email::update_email;
use crate::myffme::license::LicenseFees;
use crate::order::{
    BaseLicensePrice, EquipmentRental, InsuranceLevel, InsuranceOption, Keyed, Priced,
};
use crate::season::{current_season, is_during_discount_period};
use crate::user::{LicenseType, Metadata};
use http_body_util::{Either, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{ALLOW, CONTENT_TYPE};
use hyper::{Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tiered_server::api::Extension;
use tiered_server::headers::{GET, GET_POST, JSON};
use tiered_server::otp::action::Action;
use tiered_server::session::SessionState;
use tiered_server::store::snapshot;
use tiered_server::user::User;
use tracing::{debug, info, warn};

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
                if path == "/prices" {
                    if request.method() != Method::GET && request.method() != Method::POST {
                        let mut response = Response::builder();
                        let headers = response.headers_mut().unwrap();
                        headers.insert(ALLOW, GET_POST);
                        info!("405 https://{server_name}/api/user/admin/prices");
                        return Some(
                            response
                                .status(StatusCode::METHOD_NOT_ALLOWED)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                    let snapshot = snapshot();
                    if SessionState::from_headers(request.headers(), &snapshot).is_admin() {
                        let base_license_price_in_cents =
                            snapshot.get::<u16>(BaseLicensePrice.key());
                        let license_types = [LicenseType::Child, LicenseType::Adult]
                            .into_iter()
                            .map(|license_type| PricedLicenseType {
                                license_type,
                                fees: snapshot.get::<LicenseFees>(license_type.key()),
                            })
                            .collect::<Vec<_>>();
                        let insurance_levels = [
                            InsuranceLevel::RC,
                            InsuranceLevel::Base,
                            InsuranceLevel::BasePlus,
                            InsuranceLevel::BasePlusPlus,
                        ]
                        .into_iter()
                        .map(|level| {
                            let price_in_cents = level.price_in_cents(&snapshot, false);
                            PricedLevel {
                                level,
                                price_in_cents,
                            }
                        })
                        .collect::<Vec<_>>();
                        let insurance_options = [
                            InsuranceOption::Ski,
                            InsuranceOption::MountainBike,
                            InsuranceOption::SlacklineAndHighline,
                            InsuranceOption::TrailRunning,
                        ]
                        .into_iter()
                        .map(|option| {
                            let price_in_cents = option.price_in_cents(&snapshot, false);
                            PricedAddon {
                                option,
                                price_in_cents,
                            }
                        })
                        .collect::<Vec<_>>();
                        let equipment_rental_price_in_cents =
                            EquipmentRental.price_in_cents(&snapshot, false);
                        info!("200 https://{server_name}/api/user/admin/prices");
                        return Some(
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(CONTENT_TYPE, JSON)
                                .body(Either::Left(Full::from(
                                    serde_json::to_vec(&AdminPrices {
                                        base_license_price_in_cents,
                                        license_types,
                                        insurance_levels,
                                        insurance_options,
                                        equipment_rental_price_in_cents,
                                    })
                                    .unwrap(),
                                )))
                                .unwrap(),
                        );
                    } else {
                        info!("403 https://{server_name}/api/user/admin/prices");
                        return Some(
                            Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(Either::Right(Empty::new()))
                                .unwrap(),
                        );
                    }
                }
            } else if path == "/prices" {
                if request.method() != Method::GET {
                    let mut response = Response::builder();
                    let headers = response.headers_mut().unwrap();
                    headers.insert(ALLOW, GET);
                    info!("405 https://{server_name}/api/user/prices");
                    return Some(
                        response
                            .status(StatusCode::METHOD_NOT_ALLOWED)
                            .body(Either::Right(Empty::new()))
                            .unwrap(),
                    );
                }
                let snapshot = snapshot();
                if let SessionState::Valid { user, .. } =
                    SessionState::from_headers(request.headers(), &snapshot)
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
                    info!("200 https://{server_name}/api/user/prices");
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
                    info!("403 https://{server_name}/api/user/prices");
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
    async fn perform_action(
        &self,
        user: &User,
        action: Action,
        value: Option<&Value>,
    ) -> Option<()> {
        match action {
            Action::EmailUpdate => {
                debug!("email update");
                if let Some(ref myffme_user_id) = user.metadata.as_ref().and_then(|value| {
                    Metadata::deserialize(value)
                        .map_err(|err| {
                            warn!("failed to deserialize metadata: {:?}", err);
                        })
                        .ok()
                        .and_then(|it| it.myffme_user_id)
                }) {
                    let email = value.and_then(|value| {
                        #[derive(Deserialize)]
                        struct NewEmail {
                            new_email: String,
                        }
                        NewEmail::deserialize(value)
                            .map(|it| it.new_email)
                            .map_err(|err| {
                                warn!("failed to deserialize email: {:?}", err);
                                err
                            })
                            .ok()
                    })?;
                    return if update_email(myffme_user_id, &email, user.email())
                        .await
                        .is_some()
                    {
                        Some(())
                    } else {
                        warn!("failed to update email");
                        None
                    };
                }
            }
            _ => {}
        }
        Some(())
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

#[derive(Serialize)]
struct AdminPrices {
    base_license_price_in_cents: Option<u16>,
    license_types: Vec<PricedLicenseType>,
    insurance_levels: Vec<PricedLevel>,
    insurance_options: Vec<PricedAddon>,
    equipment_rental_price_in_cents: u16,
}

#[derive(Serialize)]
struct PricedLicenseType {
    license_type: LicenseType,
    fees: Option<LicenseFees>,
}

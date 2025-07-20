use crate::http_client::json_client;
use crate::myffme::address::Address;
use crate::myffme::graphql::address::user_addresses;
use crate::myffme::graphql::document::Document;
use crate::myffme::graphql::health_questionnaire::user_health_questionnaires;
use crate::myffme::graphql::license::user_licenses;
use crate::myffme::graphql::medical_certificate::user_medical_certificates;
use crate::myffme::graphql::structure::{structure_licenses, structures_by_ids};
use crate::myffme::LicenseType::NonPracticing;
use crate::myffme::{
    Gender, License, MedicalCertificateStatus, Member, Metadata, Structure, ADMIN,
    MYFFME_AUTHORIZATION, X_HASURA_ROLE,
};
use crate::season::current_season;
use reqwest::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::{Response, Url};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::str::from_utf8;
use tiered_server::norm::{normalize_first_name, normalize_last_name};
#[cfg(test)]
use tokio::io::AsyncWriteExt;

pub async fn members_by_ids(ids: &[&str], season: Option<u16>) -> Option<Vec<Member>> {
    let season = season.unwrap_or_else(|| current_season(None));
    let response = users_response_by_ids(ids).await?;
    users_response_to_members(response, season).await
}

pub async fn member_by_name_and_dob(
    first_name: &str,
    last_name: &str,
    dob: u32,
) -> Option<Vec<Member>> {
    let response = users_response_by_dob(dob).await?;
    let mut results = users_response_to_members(response, current_season(None)).await?;
    let normalized_first_name = normalize_first_name(first_name);
    let normalized_last_name = normalize_last_name(last_name);

    if results.len() > 1 {
        let found = results
            .iter()
            .filter_map(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    if normalize_first_name(&it.first_name) == normalized_first_name {
                        return Some(license_number);
                    }
                }
                None
            })
            .collect::<BTreeSet<_>>();
        if !found.is_empty() {
            results.retain(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    found.contains(&license_number)
                } else {
                    false
                }
            });
        }
        let found = results
            .iter()
            .filter_map(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    if normalize_first_name(&it.last_name) == normalized_last_name {
                        return Some(license_number);
                    }
                }
                None
            })
            .collect::<BTreeSet<_>>();
        if !found.is_empty() {
            results.retain(|it| {
                if let Some(license_number) = it.metadata.license_number {
                    found.contains(&license_number)
                } else {
                    false
                }
            });
        }
    }
    Some(results)
}

pub async fn member_by_license_number(license_number: u32) -> Option<Member> {
    let response = users_response_by_license_numbers(&[license_number]).await?;
    let mut iter = users_response_to_members(response, current_season(None))
        .await?
        .into_iter();
    let first = iter.next()?;
    if iter.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub async fn members_by_structure(structure_id: u32, season: Option<u16>) -> Option<Vec<Member>> {
    let season = season.unwrap_or_else(|| current_season(None));
    let response = users_response_by_structure(structure_id).await?;
    users_response_to_members(response, season).await
}

pub async fn licensees(structure_id: u32, season: u16) -> Option<Vec<Member>> {
    let licenses = structure_licenses(structure_id, season).await?;
    let user_ids = licenses.keys().map(|it| it.as_str()).collect::<Vec<_>>();
    let response = users_response_by_ids(&user_ids).await?;
    #[cfg(test)]
    let users = {
        println!("users");
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".licensees_{structure_id}_{season}.json");
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|err| {
                eprintln!("{err:?}");
                err
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let users = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    let addresses = user_addresses(&user_ids).await?;
    let medical_certificates = user_medical_certificates(&user_ids, season).await?;
    let health_questionnaires = user_health_questionnaires(&user_ids, season).await?;
    let structure_ids = licenses
        .values()
        .map(|it| it.structure_id)
        .collect::<Vec<_>>();
    let structures = structures_by_ids(&structure_ids).await?;
    Some(members(
        users,
        licenses,
        addresses,
        medical_certificates,
        health_questionnaires,
        structures,
    ))
}

#[derive(Deserialize)]
struct User {
    pub id: String,
    #[serde(deserialize_with = "deserialize_gender")]
    pub gender: Gender,
    pub first_name: String,
    pub last_name: String,
    // pub birth_name: Option<String>,
    #[serde(deserialize_with = "deserialize_date")]
    pub dob: u32,
    pub email: Option<String>,
    pub alt_email: Option<String>,
    // pub phone_number: Option<String>,
    // pub alt_phone_number: Option<String>,
    pub license_number: u32,
    pub non_practicing: Option<bool>,
}
#[derive(Deserialize)]
struct UserList {
    list: Vec<User>,
}
#[derive(Deserialize)]
struct GraphqlResponse {
    data: UserList,
}

async fn users_response_by_dob(dob: u32) -> Option<Response> {
    let s = dob.to_string();
    let dob = s.as_bytes();
    let yyyy = from_utf8(&dob[..4]).unwrap();
    let mm = from_utf8(&dob[4..6]).unwrap();
    let dd = from_utf8(&dob[6..]).unwrap();
    let dob = format!("{yyyy}-{mm}-{dd}");
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByDateOfBirth",
            "query": GRAPHQL_GET_USERS_BY_DATE_OF_BIRTH,
            "variables": {
                "dob": dob
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_by_ids(ids: &[&str]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByIds",
            "query": GRAPHQL_GET_USERS_BY_IDS,
            "variables": {
                "ids": ids
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()
}

async fn users_response_by_license_numbers(license_numbers: &[u32]) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByLicenseNumbers",
            "query": GRAPHQL_GET_USERS_BY_LICENSE_NUMBERS,
            "variables": {
                "license_numbers": license_numbers
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()
}

fn members(
    users: Vec<User>,
    mut licenses: BTreeMap<String, License>,
    mut addresses: BTreeMap<String, Address>,
    mut medical_certificates: BTreeMap<String, Document>,
    mut health_questionnaires: BTreeMap<String, Document>,
    structures: BTreeMap<u32, Structure>,
) -> Vec<Member> {
    users
        .into_iter()
        .map(|it| {
            let license = licenses.remove(&it.id);
            let address = addresses.remove(&it.id);
            let latest_structure = license
                .as_ref()
                .and_then(|it| structures.get(&it.structure_id).cloned());
            let latest_license_season = license.as_ref().map(|it| it.season);
            let license_type = if it.non_practicing.unwrap_or(false) {
                Some(NonPracticing)
            } else {
                license.and_then(|it| it.license_type)
            };
            let health_questionnaire = health_questionnaires.remove(&it.id).and_then(|it| {
                if let Some(season) = latest_license_season {
                    if it.season == season { Some(it) } else { None }
                } else {
                    None
                }
            });
            let medical_certificate = medical_certificates.remove(&it.id).unwrap_or(Document {
                user_id: None,
                season: 0,
                category: 5,
            });
            let medical_certificate_status =
                latest_license_season.map(|season| match medical_certificate.category {
                    5 => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Recreational
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                if medical_certificate.season + 3 > season {
                                    MedicalCertificateStatus::Recreational
                                } else {
                                    MedicalCertificateStatus::HealthQuestionnaire
                                }
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                    9 => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Competition
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                if medical_certificate.season + 3 > season {
                                    MedicalCertificateStatus::Competition
                                } else {
                                    MedicalCertificateStatus::HealthQuestionnaire
                                }
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                    _ => {
                        if medical_certificate.season == season {
                            MedicalCertificateStatus::Recreational
                        } else if let Some(questionnaire) = health_questionnaire {
                            if questionnaire.season == season {
                                MedicalCertificateStatus::HealthQuestionnaire
                            } else {
                                MedicalCertificateStatus::WaitingForDocument
                            }
                        } else {
                            MedicalCertificateStatus::WaitingForDocument
                        }
                    }
                });
            let (insee, city, zip_code) = address
                .map(|it| (it.insee, it.city, it.zip_code))
                .unwrap_or((None, None, None));
            Member {
                first_name: it.first_name,
                last_name: it.last_name,
                email: it.email.unwrap_or_else(|| it.alt_email.unwrap()),
                dob: it.dob,
                metadata: Metadata {
                    myffme_user_id: Some(it.id),
                    license_number: Some(it.license_number),
                    gender: Some(it.gender),
                    insee,
                    city,
                    zip_code,
                    license_type,
                    medical_certificate_status,
                    latest_license_season,
                    latest_structure,
                    ..Default::default()
                },
            }
        })
        .collect()
}

async fn users_response_to_members(response: Response, season: u16) -> Option<Vec<Member>> {
    #[cfg(test)]
    let users = {
        println!("users");
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = ".users.json";
        tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(file_name)
            .await
            .ok()?
            .write_all(text.as_bytes())
            .await
            .unwrap();
        serde_json::from_str::<GraphqlResponse>(&text)
            .map_err(|err| {
                eprintln!("{err:?}");
                err
            })
            .ok()?
            .data
            .list
    };
    #[cfg(not(test))]
    let users = response
        .json::<GraphqlResponse>()
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()?
        .data
        .list;
    let user_ids = users.iter().map(|it| it.id.as_str()).collect::<Vec<_>>();
    let licenses = user_licenses(&user_ids, season).await?;
    let addresses = user_addresses(&user_ids).await?;
    let medical_certificates = user_medical_certificates(&user_ids, season).await?;
    let health_questionnaires = user_health_questionnaires(&user_ids, season).await?;
    let structure_ids = licenses
        .values()
        .map(|it| it.structure_id)
        .collect::<Vec<_>>();
    let structures = structures_by_ids(&structure_ids).await?;
    Some(members(
        users,
        licenses,
        addresses,
        medical_certificates,
        health_questionnaires,
        structures,
    ))
}

async fn users_response_by_structure(structure_id: u32) -> Option<Response> {
    let url = Url::parse("https://back-prod.core.myffme.fr/v1/graphql").unwrap();
    let client = json_client();
    let request = client
        .post(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://www.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://www.myffme.fr/"))
        .header(X_HASURA_ROLE, ADMIN)
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .json(&json!({
            "operationName": "getUsersByStructureId",
            "query": GRAPHQL_GET_USERS_BY_STRUCTURE_ID,
            "variables": {
                "id": structure_id
            }
        }))
        .build()
        .ok()?;
    client
        .execute(request)
        .await
        .map_err(|err| {
            tracing::warn!("{err:?}");
            err
        })
        .ok()
}

fn deserialize_gender<'de, D>(deserializer: D) -> Result<Gender, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let n: u8 = serde::Deserialize::deserialize(deserializer)?;
    match n {
        0 => Ok(Gender::Female),
        1 => Ok(Gender::Male),
        _ => Err(serde::de::Error::custom("invalid gender")),
    }
}

fn deserialize_date<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    let date = s
        .split('T')
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let mut split = date.split('-');
    let yyyy = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let mm = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    let dd = split
        .next()
        .ok_or_else(|| serde::de::Error::custom("invalid date"))?;
    format!("{yyyy}{mm}{dd}")
        .parse()
        .map_err(serde::de::Error::custom)
}

const GRAPHQL_GET_USERS_BY_IDS: &str = "\
    query getUsersByIds(
        $ids: [uuid!]!
    ) {
        list: UTI_Utilisateurs(
            where: { id: { _in: $ids } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_DATE_OF_BIRTH: &str = "\
    query getUsersByDateOfBirth(
        $dob: date!
    ) {
        list: UTI_Utilisateurs(
            where: { DN_DateNaissance: { _eq: $dob } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_LICENSE_NUMBERS: &str = "\
    query getUsersByLicenseNumbers(
        $license_numbers: [bigint!]!
    ) {
        list: UTI_Utilisateurs(
            where: { LicenceNumero: { _in: $license_numbers } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

const GRAPHQL_GET_USERS_BY_STRUCTURE_ID: &str = "\
    query getUsersByStructureId(
        $id: Int!
    ) {
        list: UTI_Utilisateurs(
            where: { STR_StructureUtilisateurs: { ID_Structure: { _eq: $id} } }
        ) {
            id
            gender: CT_EST_Civilite
            first_name: CT_Prenom
            last_name: CT_Nom
            birth_name: CT_NomDeNaissance
            dob: DN_DateNaissance
            email: CT_Email
            alt_email: CT_Email2
            phone_number: CT_TelephoneMobile
            alt_phone_number: CT_TelephoneFixe
            license_number: LicenceNumero
            username: LOG_Login
            birth_place: DN_CommuneNaissance
            birth_place_insee: DN_CommuneNaissance_CodeInsee
        }
    }\
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::update_myffme_bearer_token;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_member_by_license_number() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let result = member_by_license_number(154316).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(19750826, result.dob);
        assert_eq!("GRAS", result.last_name);
    }

    #[tokio::test]
    async fn test_licensee_by_last_name_and_dob() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = member_by_name_and_dob("Jerome", "DAVID", 19770522)
            .await
            .unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert!(!results.is_empty());
        println!("{}", results.len());
        let result = results.first().unwrap();
        assert_eq!(19770522, result.dob);
        assert_eq!("DAVID", result.last_name);
    }

    #[tokio::test]
    async fn test_member_by_ids() {
        assert!(update_myffme_bearer_token(0).await.is_some());
        let t0 = SystemTime::now();
        let results = members_by_ids(
            [
                "6692903b-8032-43ea-8cd9-530f14bf5324",
                "5f5e0d27-cf50-42ea-89f8-f1649a2ef6aa",
            ]
            .as_slice(),
            None,
        )
        .await
        .unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        assert_eq!(2, results.len());
        let result = results
            .iter()
            .find(|&it| it.metadata.license_number == Some(33109))
            .unwrap();
        assert_eq!(19770522, result.dob);
        assert_eq!("DAVID", result.last_name);
    }
}

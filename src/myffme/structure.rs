use crate::http_client::json_client;
use crate::myffme::{Structure, MYFFME_AUTHORIZATION};
use hyper::header::{HeaderValue, AUTHORIZATION, ORIGIN, REFERER};
use reqwest::Url;
use serde::Deserialize;
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
pub(crate) struct StructureHierarchy {
    #[allow(dead_code)]
    pub id: u32,
    #[serde(alias = "ct", deserialize_with = "deserialize_id")]
    pub department_structure_id: u32,
    #[serde(alias = "ligue", deserialize_with = "deserialize_id")]
    pub region_structure_id: u32,
    #[serde(alias = "ffme", deserialize_with = "deserialize_id")]
    pub national_structure_id: u32,
    #[serde(rename = "label")]
    pub name: String,
    #[serde(rename = "slug")]
    pub code: String,
    pub department: Option<Department>,
}

#[derive(Deserialize)]
pub(crate) struct Department {
    #[serde(rename = "id")]
    pub number: String,
    #[serde(rename = "label")]
    pub name: String,
}

impl From<StructureHierarchy> for Structure {
    fn from(value: StructureHierarchy) -> Self {
        let StructureHierarchy {
            id,
            name,
            code,
            department,
            ..
        } = value;
        Self {
            id,
            name,
            code: Some(code),
            department: department.map(|it| format!("{} ({})", it.name, it.number)),
        }
    }
}

#[allow(dead_code)]
pub async fn structure_hierarchy_by_id(id: u32) -> Option<StructureHierarchy> {
    let url = Url::parse(&format!("https://api.core.myffme.fr/api/structures/{id}")).unwrap();
    let client = json_client();
    let request = client
        .get(url.as_str())
        .header(ORIGIN, HeaderValue::from_static("https://app.myffme.fr"))
        .header(REFERER, HeaderValue::from_static("https://app.myffme.fr/"))
        .header(
            AUTHORIZATION,
            MYFFME_AUTHORIZATION
                .get_ref()
                .map(|it| it.bearer_token.clone())?,
        )
        .build()
        .ok()?;
    let response = client.execute(request).await.ok()?;

    #[cfg(test)]
    let structure_hierarchy = {
        println!("structure");
        println!("GET {}", url.as_str());
        println!("{}", response.status());
        let text = response.text().await.ok()?;
        let file_name = format!(".api/.structure_{id}.json");
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
        serde_json::from_str::<StructureHierarchy>(&text)
            .inspect_err(|err| eprintln!("{err:?}"))
            .ok()?
    };
    #[cfg(not(test))]
    let structure_hierarchy = response
        .json::<StructureHierarchy>()
        .await
        .inspect_err(|err| tracing::warn!("{err:?}"))
        .ok()?;
    Some(structure_hierarchy)
}

#[derive(Deserialize)]
struct Id {
    pub id: u32,
}

pub(crate) fn deserialize_id<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(<Id>::deserialize(deserializer)?.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::myffme::{update_myffme_bearer_token, STRUCTURE_ID};
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_structure_hierarchy_by_id() {
        assert!(update_myffme_bearer_token(0, None).await.is_some());
        let t0 = SystemTime::now();
        let hierarchy = structure_hierarchy_by_id(*STRUCTURE_ID).await.unwrap();
        let elapsed = t0.elapsed().unwrap();
        println!("{elapsed:?}");
        println!(
            "{}, {}, {}, {}",
            hierarchy.id,
            hierarchy.department_structure_id,
            hierarchy.region_structure_id,
            hierarchy.national_structure_id
        );
        assert_eq!(1318, hierarchy.national_structure_id);
    }
}

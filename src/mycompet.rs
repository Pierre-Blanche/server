use crate::http_client::html_client;
use crate::user::{Competition, CompetitionResult};
use reqwest::Url;
use scraper::{Html, Selector};
use tracing::warn;

pub async fn competition_results(license_number: u32) -> Option<Vec<CompetitionResult>> {
    let client = html_client();
    let request = client
        .get(
            Url::parse(&format!(
                "https://mycompet.ffme.fr/resultat/palmares_{license_number:0>6}"
            ))
            .unwrap(),
        )
        .build()
        .ok()?;
    let response = client
        .execute(request)
        .await
        .map_err(|err| {
            eprintln!("{err:?}");
            err
        })
        .ok()?;
    if !response.status().is_success() {
        warn!("failed to get competition results for license number {license_number}");
        return None;
    }
    let text = response.text().await.ok()?;
    let document = Html::parse_document(text.as_str());
    let table = document
        .select(&Selector::parse("#resultats-content .index-table").unwrap())
        .next()?;
    let mut season_index = None;
    let mut competition_name_index = None;
    let mut category_name_index = None;
    let mut rank_index = None;
    for (i, header) in table
        .select(&Selector::parse("thead tr:first-of-type :is(td,th)").unwrap())
        .enumerate()
    {
        let text = header.text().collect::<String>();
        match text.as_str() {
            "Saison" => season_index = Some(i),
            "Compétition" => competition_name_index = Some(i),
            "Catégorie" => category_name_index = Some(i),
            "Rang" => rank_index = Some(i),
            _ => {}
        };
    }
    if season_index.is_none() {
        warn!("failed to find column header for competition season");
        return None;
    }
    let season_index = season_index.unwrap();
    if competition_name_index.is_none() {
        warn!("failed to find column header for competition name");
        return None;
    }
    let competition_name_index = competition_name_index.unwrap();
    if category_name_index.is_none() {
        warn!("failed to find column header for competition category");
        return None;
    }
    let category_name_index = category_name_index.unwrap();
    if rank_index.is_none() {
        warn!("failed to find column header for competition rank");
        return None;
    }
    let rank_index = rank_index.unwrap();
    let mut results = Vec::new();
    for row in table.select(&Selector::parse("tbody tr").unwrap()) {
        let mut season = None;
        let mut competition_name = None;
        let mut category_name = None;
        let mut rank = None;
        for (i, col) in row.select(&Selector::parse("td").unwrap()).enumerate() {
            if i == season_index {
                let text = col.text().map(|it| it.trim()).collect::<String>();
                let mut split = text.split('-');
                let year = split.next();
                if year.is_none() || split.next().is_none() || split.next().is_some() {
                    warn!("failed to parse season");
                    return None;
                }
                let year = year.unwrap().parse::<u16>();
                if year.is_err() {
                    warn!("failed to parse season");
                    return None;
                }
                season = year.ok();
            } else if i == competition_name_index {
                competition_name = Some(col.text().collect::<String>().trim().to_string())
            } else if i == category_name_index {
                category_name = Some(col.text().collect::<String>().trim().to_string())
            } else if i == rank_index {
                let text = col.text().map(|it| it.trim()).collect::<String>();
                let n = text.parse::<u16>();
                if n.is_err() {
                    warn!("failed to parse rank");
                    return None;
                }
                rank = n.ok();
            }
        }
        if season.is_none() {
            warn!("failed to find column for competition season");
            return None;
        }
        let season = season.unwrap();
        if competition_name.is_none() {
            warn!("failed to find column for competition name");
            return None;
        }
        let competition_name = competition_name.unwrap();
        if category_name.is_none() {
            warn!("failed to find column for competition category");
            return None;
        }
        let category_name = category_name.unwrap();
        if rank.is_none() {
            warn!("failed to find column for competition rank");
            return None;
        }
        let rank = rank.unwrap();
        results.push(CompetitionResult {
            competition: Competition {
                season,
                name: competition_name,
            },
            category_name,
            rank,
        });
    }
    Some(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let license_number = 33109;
        assert_eq!(
            "https://mycompet.ffme.fr/resultat/palmares_033109",
            format!("https://mycompet.ffme.fr/resultat/palmares_{license_number:0>6}")
        )
    }

    #[tokio::test]
    async fn test_competition_results() {
        let results = competition_results(33109).await.unwrap();
        assert!(!results.is_empty());
        let result = results
            .into_iter()
            .find(|it| it.competition.season == 2021)
            .unwrap();
        assert_eq!(result.rank, 1);
        assert_eq!(result.category_name, "VETERAN");
    }
}

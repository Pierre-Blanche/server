use std::time::SystemTime;

const APPROXIMATE_NUMBER_OF_SECS_IN_YEAR: u32 = 31_557_600;

pub fn current_season(timestamp: Option<u32>) -> u16 {
    let year_2020_utc_start_timestamp = 1577836800_u32;
    let elapsed = timestamp.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32
    }) - year_2020_utc_start_timestamp;
    // can be off by 1 but won't change the result
    let years = elapsed / APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let current_year_elapsed_seconds = elapsed - years * APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let years = years as u16;
    let seconds_between_jan_and_august = if years % 4 == 0 {
        18_316_800
    } else {
        18_230_400
    };
    if current_year_elapsed_seconds > seconds_between_jan_and_august {
        2020 + years + 1
    } else {
        2020 + years
    }
}

pub fn is_during_discount_period(timestamp: Option<u32>) -> bool {
    let year_2020_utc_start_timestamp = 1577836800_u32;
    let elapsed = timestamp.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32
    }) - year_2020_utc_start_timestamp;
    // can be off by 1 but won't change the result
    let years = elapsed / APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let current_year_elapsed_seconds = elapsed - years * APPROXIMATE_NUMBER_OF_SECS_IN_YEAR;
    let years = years as u16;
    let seconds_between_jan_and_may = if years % 4 == 0 {
        10_540_800
    } else {
        10_454_400
    };
    let seconds_between_jan_and_august = if years % 4 == 0 {
        18_316_800
    } else {
        18_230_400
    };
    current_year_elapsed_seconds > seconds_between_jan_and_may
        && current_year_elapsed_seconds < seconds_between_jan_and_august
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::season::current_season;
    use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeZone, Utc};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_current_season() {
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 3, 12).unwrap(),
        ));
        let season = current_season(Some(date.timestamp() as u32));
        assert_eq!(2021, season);
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 9, 1).unwrap(),
        ));
        let season = current_season(Some(date.timestamp() as u32));
        assert_eq!(2022, season);
        let date = Utc
            .timestamp_millis_opt(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
            )
            .unwrap();
        let season = current_season(None);
        assert_eq!(season, current_season(Some(date.timestamp() as u32)));
        let mut year = date.year() as u16;
        let month = date.month() as u16;
        let day = date.day() as u16;
        if month == 7 && day > 29 {
            return;
        }
        if month == 8 && day < 3 {
            return;
        }
        if month == 8 {
            year += 1;
        }
        assert_eq!(year, season);
    }

    #[test]
    fn test_is_during_discount_period() {
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 3, 12).unwrap(),
        ));
        assert!(!is_during_discount_period(Some(date.timestamp() as u32)));
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2021, 1, 1).unwrap(),
        ));
        assert!(!is_during_discount_period(Some(date.timestamp() as u32)));
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
        ));
        assert!(!is_during_discount_period(Some(date.timestamp() as u32)));
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2020, 4, 30).unwrap(),
        ));
        assert!(!is_during_discount_period(Some(date.timestamp() as u32)));
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2023, 8, 1).unwrap(),
        ));
        assert!(!is_during_discount_period(Some(date.timestamp() as u32)));
        let date = Utc.from_utc_datetime(&NaiveDateTime::from(
            NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(),
        ));
        assert!(is_during_discount_period(Some(date.timestamp() as u32)));
    }
}

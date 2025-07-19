#[derive(PartialEq, PartialOrd, Ord, Eq, Copy, Clone)]
pub enum Category {
    Baby,
    U8,
    U10,
    U12,
    U14,
    U16,
    U18,
    U20,
    Seniors,
    Veterans,
}

impl Category {
    pub fn from_dob(date_of_birth: u32, season: u16) -> Self {
        let year = (date_of_birth / 1_00_00) as u16;
        let age = season - year;
        match age {
            n if n < 6 => Self::Baby,
            6 | 7 => Self::U8,
            8 | 9 => Self::U10,
            10 | 11 => Self::U12,
            12 | 13 => Self::U14,
            14 | 15 => Self::U16,
            16 | 17 => Self::U18,
            18 | 19 => Self::U20,
            n if n < 40 => Self::Seniors,
            _ => Self::Veterans,
        }
    }
}

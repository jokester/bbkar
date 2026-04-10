#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DayNumber(i64);

impl DayNumber {
    pub(crate) fn today() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Self(seconds / 86_400)
    }

    pub(crate) fn from_ymd(year: i64, month: i64, day: i64) -> Self {
        Self(day_number_from_ymd(year, month, day))
    }

    pub(crate) fn from_timestamp(timestamp: &str) -> Option<Self> {
        let (year, month, day) = parse_timestamp_ymd(timestamp)?;
        Some(Self::from_ymd(year, month, day))
    }

    pub(crate) fn into_inner(self) -> i64 {
        self.0
    }

    pub(crate) fn to_ymd_string(self) -> String {
        let z = self.0 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let year = y + if m <= 2 { 1 } else { 0 };
        format!("{year:04}{m:02}{d:02}")
    }
}

pub(crate) fn parse_date_to_days(timestamp: &str) -> Option<DayNumber> {
    DayNumber::from_timestamp(timestamp)
}

pub(crate) fn parse_timestamp_ymd(timestamp: &str) -> Option<(i64, i64, i64)> {
    if timestamp.len() < 8 {
        return None;
    }
    let date_str = &timestamp[..8];
    let year: i64 = date_str[0..4].parse().ok()?;
    let month: i64 = date_str[4..6].parse().ok()?;
    let day: i64 = date_str[6..8].parse().ok()?;
    Some((year, month, day))
}

pub(crate) fn day_number_from_ymd(year: i64, month: i64, day: i64) -> i64 {
    let adjust = if month <= 2 { 1 } else { 0 };
    let era_year = year - adjust;
    let era = if era_year >= 0 { era_year } else { era_year - 399 } / 400;
    let yoe = era_year - era * 400;
    let month_index = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * month_index + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

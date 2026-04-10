pub(crate) fn parse_date_to_days(timestamp: &str) -> Option<i64> {
    let (year, month, day) = parse_timestamp_ymd(timestamp)?;
    Some(day_number_from_ymd(year, month, day))
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

pub(crate) fn current_day_number() -> i64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    seconds / 86_400
}

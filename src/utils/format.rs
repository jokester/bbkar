use std::time::Duration;

use crate::utils::duration::{PreserveCount, TimeUnit, Weekday};

pub fn format_bytes(n: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if n >= TIB {
        format!("{:.2} TiB", n as f64 / TIB as f64)
    } else if n >= GIB {
        format!("{:.2} GiB", n as f64 / GIB as f64)
    } else if n >= MIB {
        format!("{:.2} MiB", n as f64 / MIB as f64)
    } else if n >= KIB {
        format!("{:.2} KiB", n as f64 / KIB as f64)
    } else {
        format!("{} bytes", n)
    }
}

pub fn format_speed(bytes: u64, duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 0.001 {
        return "N/A".to_string();
    }
    let bytes_per_sec = (bytes as f64 / secs) as u64;
    format!("{}/s", format_bytes(bytes_per_sec))
}

pub fn format_calendar_days(days: u32) -> String {
    match days {
        d if d % 365 == 0 => format!("{}y", d / 365),
        d if d % 30 == 0 => format!("{}m", d / 30),
        d if d % 7 == 0 => format!("{}w", d / 7),
        d => format!("{d}d"),
    }
}

pub fn format_preserve_count(count: &PreserveCount) -> String {
    match count {
        PreserveCount::All => "*".to_string(),
        PreserveCount::Finite(n) => n.to_string(),
    }
}

pub fn format_time_unit(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Day => "d",
        TimeUnit::Week => "w",
        TimeUnit::Month => "m",
        TimeUnit::Year => "y",
    }
}

pub fn format_weekday(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "monday",
        Weekday::Tuesday => "tuesday",
        Weekday::Wednesday => "wednesday",
        Weekday::Thursday => "thursday",
        Weekday::Friday => "friday",
        Weekday::Saturday => "saturday",
        Weekday::Sunday => "sunday",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::duration::{PreserveCount, TimeUnit, Weekday};

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(512), "512 bytes");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TiB");
        assert_eq!(format_bytes(1536), "1.50 KiB");
    }

    #[test]
    fn test_format_calendar_days() {
        assert_eq!(format_calendar_days(7), "1w");
        assert_eq!(format_calendar_days(30), "1m");
        assert_eq!(format_calendar_days(365), "1y");
        assert_eq!(format_calendar_days(10), "10d");
    }

    #[test]
    fn test_format_policy_helpers() {
        assert_eq!(format_preserve_count(&PreserveCount::All), "*");
        assert_eq!(format_preserve_count(&PreserveCount::Finite(12)), "12");
        assert_eq!(format_time_unit(TimeUnit::Month), "m");
        assert_eq!(format_weekday(Weekday::Sunday), "sunday");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(2048, Duration::from_secs(2)), "1.00 KiB/s");
        assert_eq!(format_speed(2048, Duration::from_nanos(1)), "N/A");
    }
}

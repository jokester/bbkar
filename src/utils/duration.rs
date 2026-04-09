/// Calendar-based duration used for backup policy intervals.
/// Approximate: d=1, w=7, m=30, y=365 (standard in backup tooling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDuration {
    pub days: u32,
}

impl CalendarDuration {
    /// Parse a duration string like "30d", "4w", "1m", "1y".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (num_str, unit_char) = s.split_at(s.len() - 1);
        let count: u32 = num_str.parse().ok()?;
        let multiplier = match unit_char {
            "d" => 1,
            "w" => 7,
            "m" => 30,
            "y" => 365,
            _ => return None,
        };
        Some(CalendarDuration {
            days: count * multiplier,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreserveCount {
    Finite(u32),
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreserveBucket {
    pub count: PreserveCount,
    pub unit: TimeUnit,
}

/// Schedule-based retention, e.g. "30d 12w 6m *y".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreserveSchedule {
    pub buckets: Vec<PreserveBucket>,
}

impl PreserveSchedule {
    /// Parse a schedule string like "30d 12w 6m *y".
    /// Each token is `<count><unit>` where count is a number or `*`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut buckets = Vec::new();
        for token in s.split_whitespace() {
            if token.is_empty() {
                continue;
            }
            let (count_str, unit_char) = token.split_at(token.len() - 1);
            let unit = match unit_char {
                "d" => TimeUnit::Day,
                "w" => TimeUnit::Week,
                "m" => TimeUnit::Month,
                "y" => TimeUnit::Year,
                _ => return None,
            };
            let count = if count_str == "*" {
                PreserveCount::All
            } else {
                PreserveCount::Finite(count_str.parse().ok()?)
            };
            buckets.push(PreserveBucket { count, unit });
        }
        if buckets.is_empty() {
            return None;
        }
        Some(PreserveSchedule { buckets })
    }

    /// Return the coarsest bucket's equivalent CalendarDuration.
    /// Used to derive min_full_send_interval from archive_preserve.
    pub fn coarsest_duration(&self) -> Option<CalendarDuration> {
        self.buckets.last().map(|b| {
            let days = match b.unit {
                TimeUnit::Day => 1,
                TimeUnit::Week => 7,
                TimeUnit::Month => 30,
                TimeUnit::Year => 365,
            };
            CalendarDuration { days }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "monday" => Some(Weekday::Monday),
            "tuesday" => Some(Weekday::Tuesday),
            "wednesday" => Some(Weekday::Wednesday),
            "thursday" => Some(Weekday::Thursday),
            "friday" => Some(Weekday::Friday),
            "saturday" => Some(Weekday::Saturday),
            "sunday" => Some(Weekday::Sunday),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calendar_duration_parse() {
        assert_eq!(
            CalendarDuration::parse("30d"),
            Some(CalendarDuration { days: 30 })
        );
        assert_eq!(
            CalendarDuration::parse("4w"),
            Some(CalendarDuration { days: 28 })
        );
        assert_eq!(
            CalendarDuration::parse("1m"),
            Some(CalendarDuration { days: 30 })
        );
        assert_eq!(
            CalendarDuration::parse("1y"),
            Some(CalendarDuration { days: 365 })
        );
        assert_eq!(
            CalendarDuration::parse("2m"),
            Some(CalendarDuration { days: 60 })
        );
    }

    #[test]
    fn test_calendar_duration_invalid() {
        assert_eq!(CalendarDuration::parse(""), None);
        assert_eq!(CalendarDuration::parse("d"), None);
        assert_eq!(CalendarDuration::parse("30x"), None);
        assert_eq!(CalendarDuration::parse("abc"), None);
        assert_eq!(CalendarDuration::parse("30"), None);
    }

    #[test]
    fn test_preserve_schedule_parse() {
        let s = PreserveSchedule::parse("30d 12w 6m *y").unwrap();
        assert_eq!(s.buckets.len(), 4);
        assert_eq!(
            s.buckets[0],
            PreserveBucket {
                count: PreserveCount::Finite(30),
                unit: TimeUnit::Day
            }
        );
        assert_eq!(
            s.buckets[1],
            PreserveBucket {
                count: PreserveCount::Finite(12),
                unit: TimeUnit::Week
            }
        );
        assert_eq!(
            s.buckets[2],
            PreserveBucket {
                count: PreserveCount::Finite(6),
                unit: TimeUnit::Month
            }
        );
        assert_eq!(
            s.buckets[3],
            PreserveBucket {
                count: PreserveCount::All,
                unit: TimeUnit::Year
            }
        );
    }

    #[test]
    fn test_preserve_schedule_single() {
        let s = PreserveSchedule::parse("*y").unwrap();
        assert_eq!(s.buckets.len(), 1);
        assert_eq!(
            s.buckets[0],
            PreserveBucket {
                count: PreserveCount::All,
                unit: TimeUnit::Year
            }
        );
    }

    #[test]
    fn test_preserve_schedule_invalid() {
        assert_eq!(PreserveSchedule::parse(""), None);
        assert_eq!(PreserveSchedule::parse("30x"), None);
        assert_eq!(PreserveSchedule::parse("abc"), None);
    }

    #[test]
    fn test_preserve_schedule_coarsest() {
        let s = PreserveSchedule::parse("30d 12w 6m *y").unwrap();
        assert_eq!(s.coarsest_duration(), Some(CalendarDuration { days: 365 }));

        let s = PreserveSchedule::parse("30d 12w").unwrap();
        assert_eq!(s.coarsest_duration(), Some(CalendarDuration { days: 7 }));
    }

    #[test]
    fn test_weekday_parse() {
        assert_eq!(Weekday::parse("sunday"), Some(Weekday::Sunday));
        assert_eq!(Weekday::parse("Monday"), Some(Weekday::Monday));
        assert_eq!(Weekday::parse("TUESDAY"), Some(Weekday::Tuesday));
        assert_eq!(Weekday::parse("wednesday"), Some(Weekday::Wednesday));
        assert_eq!(Weekday::parse("thursday"), Some(Weekday::Thursday));
        assert_eq!(Weekday::parse("friday"), Some(Weekday::Friday));
        assert_eq!(Weekday::parse("saturday"), Some(Weekday::Saturday));
    }

    #[test]
    fn test_weekday_invalid() {
        assert_eq!(Weekday::parse(""), None);
        assert_eq!(Weekday::parse("sun"), None);
        assert_eq!(Weekday::parse("notaday"), None);
    }
}

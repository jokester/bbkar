use crate::model::config::SyncSpec;
use crate::utils::format::{
    format_calendar_days, format_preserve_count, format_time_unit, format_weekday,
};
use crate::utils::duration::{CalendarDuration, PreserveSchedule, Weekday};

/// Controls how new snapshots are sent (full vs incremental).
#[derive(Debug, Clone)]
pub struct SendPolicy {
    pub min_full_send_interval: CalendarDuration,
    /// None means no depth limit.
    pub max_incremental_depth: Option<u32>,
}

/// Controls which archives are kept during pruning.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// None means "all" (keep everything).
    pub archive_preserve_min: Option<CalendarDuration>,
    pub archive_preserve: Option<PreserveSchedule>,
    pub preserve_day_of_week: Weekday,
}

impl RetentionPolicy {
    pub fn describe(&self) -> String {
        if self.archive_preserve_min.is_none() {
            return "keep all archives".to_string();
        }

        let mut parts = vec![format!(
            "keep all archives for {}",
            format_calendar_days(self.archive_preserve_min.as_ref().unwrap().days)
        )];

        if let Some(schedule) = &self.archive_preserve {
            let buckets = schedule
                .buckets
                .iter()
                .map(|bucket| {
                    format!(
                        "{}{}",
                        format_preserve_count(&bucket.count),
                        format_time_unit(bucket.unit)
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            parts.push(format!(
                "then preserve {} (week anchor: {})",
                buckets,
                format_weekday(self.preserve_day_of_week)
            ));
        }

        parts.join(", ")
    }
}

impl SendPolicy {
    pub fn describe(&self) -> String {
        let mut parts = vec![format!(
            "full at least every {}",
            format_calendar_days(self.min_full_send_interval.days)
        )];

        match self.max_incremental_depth {
            Some(depth) => parts.push(format!("max incremental depth {}", depth)),
            None => parts.push("no incremental depth limit".to_string()),
        }

        parts.join(", ")
    }
}

/// Fully resolved policy parsed from SyncSpec config strings.
#[derive(Debug, Clone)]
pub struct ResolvedSyncPolicy {
    pub send: SendPolicy,
    pub retention: RetentionPolicy,
}

impl ResolvedSyncPolicy {
    /// Resolve raw config strings into parsed policy types.
    /// Called after validation, so unwraps on parse are safe.
    pub fn from_sync_spec(spec: &SyncSpec) -> Self {
        let archive_preserve = spec
            .archive_preserve
            .as_ref()
            .and_then(|s| PreserveSchedule::parse(s));

        // Safe to unwrap: validated in from_toml()
        let min_full_send_interval = CalendarDuration::parse(&spec.min_full_send_interval).unwrap();

        let archive_preserve_min = if spec.archive_preserve_min == "all" {
            None
        } else {
            CalendarDuration::parse(&spec.archive_preserve_min)
        };

        let preserve_day_of_week =
            Weekday::parse(&spec.preserve_day_of_week).unwrap_or(Weekday::Sunday);

        ResolvedSyncPolicy {
            send: SendPolicy {
                min_full_send_interval,
                max_incremental_depth: spec.max_incremental_depth,
            },
            retention: RetentionPolicy {
                archive_preserve_min,
                archive_preserve,
                preserve_day_of_week,
            },
        }
    }
}

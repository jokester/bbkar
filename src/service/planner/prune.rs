use std::collections::HashMap;

use crate::model::dest::{DestMeta, VolumeArchive};
use crate::model::plan::{PruneDecision, PrunePlan, PruneReason, PruneStep};
use crate::model::policy::RetentionPolicy;
use crate::utils::duration::{PreserveCount, TimeUnit, Weekday};

use super::time::{current_day_number, day_number_from_ymd, parse_timestamp_ymd};

pub(crate) fn build_prune_plan(meta: Option<&DestMeta>, retention_policy: &RetentionPolicy) -> PrunePlan {
    build_prune_plan_at(meta, retention_policy, current_day_number())
}

pub(crate) fn build_prune_plan_at(
    meta: Option<&DestMeta>,
    retention_policy: &RetentionPolicy,
    now_days: i64,
) -> PrunePlan {
    let Some(meta) = meta else {
        return PrunePlan {
            decisions: vec![],
            steps: vec![],
            resulting_meta: None,
        };
    };

    let archives = meta.archives();
    let mut reasons: HashMap<String, PruneReason> = HashMap::new();

    if archives.is_empty() {
        return PrunePlan {
            decisions: vec![],
            steps: vec![],
            resulting_meta: Some(meta.clone()),
        };
    }

    if retention_policy.archive_preserve_min.is_none() {
        for archive in archives {
            reasons.insert(archive.timestamp.raw().to_string(), PruneReason::KeepAll);
        }
        return finalize_prune_plan(meta, reasons);
    }

    let archive_infos: Vec<ArchiveInfo<'_>> = archives
        .iter()
        .filter_map(|archive| {
            let Some((year, month, day)) = parse_timestamp_ymd(archive.timestamp.raw()) else {
                // Pruning should fail safe. If a timestamp cannot be bucketed,
                // keep it visible in metadata instead of risking accidental deletion.
                reasons.insert(archive.timestamp.raw().to_string(), PruneReason::KeepAll);
                return None;
            };
            Some(ArchiveInfo {
                archive,
                day_number: day_number_from_ymd(year, month, day),
                year,
                month,
            })
        })
        .collect();

    let keep_min_days = retention_policy
        .archive_preserve_min
        .as_ref()
        .map(|d| d.days as i64)
        .unwrap_or(0);

    for info in &archive_infos {
        if now_days - info.day_number < keep_min_days {
            reasons.insert(info.archive.timestamp.raw().to_string(), PruneReason::TooNew);
        }
    }

    if let Some(schedule) = &retention_policy.archive_preserve {
        for bucket in &schedule.buckets {
            apply_schedule_bucket(
                &archive_infos,
                &mut reasons,
                bucket.unit,
                &bucket.count,
                retention_policy.preserve_day_of_week,
                now_days,
            );
        }
    }

    let archive_map: HashMap<&str, &VolumeArchive> = archives.iter().map(|a| (a.timestamp.raw(), a)).collect();
    let kept: Vec<String> = reasons.keys().cloned().collect();
    for raw in kept {
        mark_required_ancestors(&raw, &archive_map, &mut reasons);
    }

    finalize_prune_plan(meta, reasons)
}

struct ArchiveInfo<'a> {
    archive: &'a VolumeArchive,
    day_number: i64,
    year: i64,
    month: i64,
}

fn apply_schedule_bucket(
    archives: &[ArchiveInfo<'_>],
    reasons: &mut HashMap<String, PruneReason>,
    unit: TimeUnit,
    count: &PreserveCount,
    preserve_day_of_week: Weekday,
    now_days: i64,
) {
    let current_period = period_key_from_day(now_days, unit, preserve_day_of_week);
    let mut earliest_per_period: HashMap<i64, &ArchiveInfo<'_>> = HashMap::new();

    for info in archives {
        let period = period_key_for_archive(info, unit, preserve_day_of_week);
        let in_range = match count {
            PreserveCount::All => true,
            PreserveCount::Finite(limit) => current_period - period < *limit as i64,
        };
        if !in_range {
            continue;
        }

        earliest_per_period
            .entry(period)
            .and_modify(|existing| {
                if info.archive.timestamp < existing.archive.timestamp {
                    *existing = info;
                }
            })
            .or_insert(info);
    }

    let reason = match unit {
        TimeUnit::Day => PruneReason::PreserveDay,
        TimeUnit::Week => PruneReason::PreserveWeek,
        TimeUnit::Month => PruneReason::PreserveMonth,
        TimeUnit::Year => PruneReason::PreserveYear,
    };

    for info in earliest_per_period.values() {
        reasons.entry(info.archive.timestamp.raw().to_string()).or_insert(reason.clone());
    }
}

fn period_key_for_archive(info: &ArchiveInfo<'_>, unit: TimeUnit, preserve_day_of_week: Weekday) -> i64 {
    match unit {
        TimeUnit::Day => info.day_number,
        TimeUnit::Week => period_key_from_day(info.day_number, unit, preserve_day_of_week),
        TimeUnit::Month => info.year * 12 + (info.month - 1),
        TimeUnit::Year => info.year,
    }
}

fn period_key_from_day(day_number: i64, unit: TimeUnit, preserve_day_of_week: Weekday) -> i64 {
    match unit {
        TimeUnit::Day => day_number,
        TimeUnit::Week => {
            let weekday = weekday_from_day_number(day_number);
            let start = weekday_index(preserve_day_of_week);
            let offset = (weekday - start).rem_euclid(7);
            (day_number - offset) / 7
        }
        TimeUnit::Month | TimeUnit::Year => unreachable!("month/year period requires parsed Y/M"),
    }
}

fn weekday_from_day_number(day_number: i64) -> i64 {
    (day_number + 3).rem_euclid(7)
}

fn weekday_index(weekday: Weekday) -> i64 {
    match weekday {
        Weekday::Monday => 0,
        Weekday::Tuesday => 1,
        Weekday::Wednesday => 2,
        Weekday::Thursday => 3,
        Weekday::Friday => 4,
        Weekday::Saturday => 5,
        Weekday::Sunday => 6,
    }
}

fn mark_required_ancestors(
    raw: &str,
    archive_map: &HashMap<&str, &VolumeArchive>,
    reasons: &mut HashMap<String, PruneReason>,
) {
    let mut current = archive_map.get(raw).copied();
    while let Some(archive) = current {
        let Some(parent_raw) = archive.parent_timestamp.as_deref() else {
            break;
        };
        match reasons.get(parent_raw) {
            Some(existing) if *existing != PruneReason::PruneCandidate => {}
            _ => {
                reasons.insert(parent_raw.to_string(), PruneReason::RequiredAncestor);
            }
        }
        current = archive_map.get(parent_raw).copied();
    }
}

fn finalize_prune_plan(meta: &DestMeta, mut reasons: HashMap<String, PruneReason>) -> PrunePlan {
    let decisions: Vec<PruneDecision> = meta
        .archives()
        .iter()
        .map(|archive| PruneDecision {
            snapshot: archive.timestamp.clone(),
            reason: reasons
                .remove(archive.timestamp.raw())
                .unwrap_or(PruneReason::PruneCandidate),
        })
        .collect();

    let kept_archives: Vec<VolumeArchive> = meta
        .archives()
        .iter()
        .zip(decisions.iter())
        .filter_map(|(archive, decision)| if decision.would_prune() { None } else { Some(archive.clone()) })
        .collect();
    let pruned_archives: Vec<VolumeArchive> = meta
        .archives()
        .iter()
        .zip(decisions.iter())
        .filter_map(|(archive, decision)| if decision.would_prune() { Some(archive.clone()) } else { None })
        .collect();

    let resulting_meta = DestMeta::new(meta.first_sync_timestamp, meta.last_sync_timestamp, kept_archives);

    let mut steps = Vec::new();
    if !pruned_archives.is_empty() {
        // The metadata write is deliberately ordered first. If later file deletion
        // fails, bbkar still presents the pruned set atomically to the user and can
        // retry orphan cleanup on a future prune run.
        steps.push(PruneStep::CommitMetadata(resulting_meta.clone()));
        steps.extend(pruned_archives.into_iter().map(PruneStep::DeleteArchive));
    }

    PrunePlan {
        decisions,
        steps,
        resulting_meta: Some(resulting_meta),
    }
}

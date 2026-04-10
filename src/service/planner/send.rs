use std::collections::{HashMap, HashSet};

use crate::DestSpec;
use crate::model::dest::{DestState, VolumeArchive};
use crate::model::error::BR;
use crate::model::plan::{RunPlan, RunStep};
use crate::model::policy::SendPolicy;
use crate::model::source::Timestamp;
use crate::service::executor::inspect_source::SourceState;
use tracing::{debug, trace};

use super::time::parse_date_to_days;

pub(crate) fn build_send_plan(
    volume: &str,
    src_state: &SourceState,
    dest_spec: &DestSpec,
    dest_state: &DestState,
    send_policy: &SendPolicy,
) -> BR<RunPlan> {
    let archives = dest_state.meta.as_ref().map(|m| m.archives()).unwrap_or(&[]);

    let mut dest_set: HashSet<&str> = archives.iter().map(|a| a.timestamp.raw()).collect();
    let archive_map: HashMap<&str, _> = archives.iter().map(|a| (a.timestamp.raw(), a)).collect();

    let mut depth_cache: HashMap<&str, u32> = HashMap::new();
    for a in archives {
        compute_depth(a.timestamp.raw(), &archive_map, &mut depth_cache);
    }

    let src_names: HashSet<&str> = src_state.volume.snapshots().iter().map(|s| s.raw()).collect();
    let mut last_full: Option<&Timestamp> = archives
        .iter()
        .rev()
        .find(|a| !a.is_incremental())
        .map(|a| &a.timestamp);

    let mut steps = Vec::new();
    debug!(
        volume = %volume,
        source_snapshots = src_state.volume.snapshots().len(),
        existing_archives = archives.len(),
        dest = %dest_spec.display_location(),
        max_incremental_depth = ?send_policy.max_incremental_depth,
        min_full_send_interval_days = send_policy.min_full_send_interval.days,
        last_full = last_full.map(|s| s.raw()),
        "building plan"
    );

    for snap in src_state.volume.snapshots().iter() {
        if dest_set.contains(snap.raw()) {
            trace!(volume = %volume, snapshot = %snap.raw(), "snapshot already archived");
            continue;
        }

        let parent = find_parent_candidate(snap, &dest_set, &src_names, src_state);
        let send_full = match parent {
            None => true,
            Some(parent_snap) => {
                let parent_depth = depth_cache.get(parent_snap.raw()).copied().unwrap_or(0);
                let would_exceed_depth = send_policy
                    .max_incremental_depth
                    .is_some_and(|max| parent_depth + 1 >= max);
                let interval_exceeded =
                    check_interval_exceeded(last_full, snap, &send_policy.min_full_send_interval);
                would_exceed_depth || interval_exceeded
            }
        };

        if send_full {
            debug!(
                volume = %volume,
                snapshot = %snap.raw(),
                parent = parent.map(|p| p.raw()),
                reason = if parent.is_none() { "no_parent" } else { "policy_forced_full" },
                "planning full send"
            );
            steps.push(RunStep::SendFull(snap.clone()));
            last_full = Some(snap);
            depth_cache.insert(snap.raw(), 0);
        } else {
            let parent_snap = parent.unwrap();
            let parent_depth = depth_cache.get(parent_snap.raw()).copied().unwrap_or(0);
            debug!(
                volume = %volume,
                snapshot = %snap.raw(),
                parent = %parent_snap.raw(),
                resulting_depth = parent_depth + 1,
                "planning incremental send"
            );
            steps.push(RunStep::SendIncremental(snap.clone(), parent_snap.clone()));
            depth_cache.insert(snap.raw(), parent_depth + 1);
        }
        dest_set.insert(snap.raw());
    }

    debug!(volume = %volume, planned_steps = steps.len(), "plan built");
    Ok(RunPlan { steps })
}

fn compute_depth<'a>(
    name: &'a str,
    archive_map: &HashMap<&'a str, &'a VolumeArchive>,
    cache: &mut HashMap<&'a str, u32>,
) -> u32 {
    if let Some(&d) = cache.get(name) {
        return d;
    }
    let depth = match archive_map.get(name) {
        Some(archive) => match &archive.parent_timestamp {
            None => 0,
            Some(parent) => compute_depth(parent.as_str(), archive_map, cache) + 1,
        },
        None => 0,
    };
    cache.insert(name, depth);
    depth
}

fn find_parent_candidate<'a>(
    snap: &Timestamp,
    dest_set: &HashSet<&str>,
    src_names: &HashSet<&str>,
    src_state: &'a SourceState,
) -> Option<&'a Timestamp> {
    src_state
        .volume
        .snapshots()
        .iter()
        .rev()
        .filter(|s| s < &snap)
        .find(|s| dest_set.contains(s.raw()) && src_names.contains(s.raw()))
}

fn check_interval_exceeded(
    last_full: Option<&Timestamp>,
    current: &Timestamp,
    interval: &crate::utils::duration::CalendarDuration,
) -> bool {
    let Some(last) = last_full else {
        return true;
    };
    let Some(last_days) = parse_date_to_days(last.timestamp()) else {
        return true;
    };
    let Some(current_days) = parse_date_to_days(current.timestamp()) else {
        return true;
    };
    (current_days - last_days) >= interval.days as i64
}

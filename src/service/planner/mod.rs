use std::collections::{HashMap, HashSet};

use crate::DestSpec;
use crate::model::dest::DestState;
use crate::model::dest::VolumeArchive;
use crate::model::error::BR;
use crate::model::plan::{RestorePlan, RestoreStep, RunPlan, RunStep};
use crate::model::policy::SendPolicy;
use crate::model::source::Timestamp;
use crate::service::executor::inspect_source::SourceState;
use tracing::{debug, trace};

/**
 * A planner provides the strategy to sync source and destination
 * Itself contains only non-IO computations. Running a Plan requires an Executor
 */
pub struct Planner;
impl Planner {
    pub fn build_plan(
        &self,
        volume: &str,
        src_state: &SourceState,
        dest_spec: &DestSpec,
        dest_state: &DestState,
        send_policy: &SendPolicy,
    ) -> BR<RunPlan> {
        // Collect snapshot names already archived in dest
        let archives = dest_state
            .meta
            .as_ref()
            .map(|m| m.archives())
            .unwrap_or(&[]);

        let mut dest_set: HashSet<&str> = archives.iter().map(|a| a.timestamp.raw()).collect();

        // Build archive map: snapshot_name -> &VolumeArchive
        let archive_map: HashMap<&str, _> =
            archives.iter().map(|a| (a.timestamp.raw(), a)).collect();

        // Compute depth cache from existing dest archives
        let mut depth_cache: HashMap<&str, u32> = HashMap::new();
        for a in archives {
            compute_depth(a.timestamp.raw(), &archive_map, &mut depth_cache);
        }

        // Source snapshot names as a set for quick lookup
        let src_names: HashSet<&str> = src_state
            .volume
            .snapshots()
            .iter()
            .map(|s| s.raw())
            .collect();

        // Track last full send (from dest archives, most recent)
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

            // Find parent candidate: most recent snapshot older than current
            // that exists in BOTH source and dest (including planned sends)
            let parent = find_parent_candidate(snap, &dest_set, &src_names, src_state);

            let send_full = match parent {
                None => true,
                Some(parent_snap) => {
                    let parent_depth = depth_cache.get(parent_snap.raw()).copied().unwrap_or(0);
                    let would_exceed_depth = send_policy
                        .max_incremental_depth
                        .is_some_and(|max| parent_depth + 1 >= max);
                    let interval_exceeded = check_interval_exceeded(
                        last_full,
                        snap,
                        &send_policy.min_full_send_interval,
                    );
                    would_exceed_depth || interval_exceeded
                }
            };

            if send_full {
                debug!(
                    volume = %volume,
                    snapshot = %snap.raw(),
                    parent = parent.map(|p| p.raw()),
                    reason = if parent.is_none() {
                        "no_parent"
                    } else {
                        "policy_forced_full"
                    },
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

    /// Build a restore plan: walk the parent chain from `target` back to a full
    /// backup, then return steps in apply-order (full first, then incrementals).
    pub fn build_restore_plan(
        &self,
        archives: &[VolumeArchive],
        target: &Timestamp,
    ) -> BR<RestorePlan> {
        let mut chain = Vec::new();

        let target_archive = archives
            .iter()
            .find(|a| a.timestamp == *target)
            .ok_or_else(|| {
                crate::model::error::BbkarError::Plan(format!(
                    "snapshot '{}' not found in archives",
                    target.raw()
                ))
            })?;

        chain.push(target_archive);

        let mut current = target_archive;
        while let Some(ref parent_raw) = current.parent_timestamp {
            current = archives
                .iter()
                .find(|a| a.timestamp.raw() == parent_raw.as_str())
                .ok_or_else(|| {
                    crate::model::error::BbkarError::Plan(format!(
                        "parent snapshot '{}' not found in archives (required by '{}')",
                        parent_raw,
                        current.timestamp.raw()
                    ))
                })?;
            chain.push(current);
        }

        chain.reverse();

        let steps = chain
            .into_iter()
            .map(|a| {
                if let Some(ref parent) = a.parent_timestamp {
                    RestoreStep::ReceiveIncremental(
                        a.timestamp.clone(),
                        Timestamp::parse(parent).unwrap(),
                    )
                } else {
                    RestoreStep::ReceiveFull(a.timestamp.clone())
                }
            })
            .collect();

        Ok(RestorePlan { steps })
    }
}

/// Recursively compute depth: 0 = full, N = N incrementals deep.
fn compute_depth<'a>(
    name: &'a str,
    archive_map: &HashMap<&'a str, &'a crate::model::dest::VolumeArchive>,
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
        None => 0, // not found, treat as full
    };
    cache.insert(name, depth);
    depth
}

/// Find the most recent snapshot older than `snap` that exists in both source and dest
/// (including snapshots planned in current run).
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

/// Check if the interval between last_full and current snapshot >= min_full_send_interval.
fn check_interval_exceeded(
    last_full: Option<&Timestamp>,
    current: &Timestamp,
    interval: &crate::utils::duration::CalendarDuration,
) -> bool {
    let Some(last) = last_full else {
        return true; // no previous full → force full
    };
    let Some(last_days) = parse_date_to_days(last.timestamp()) else {
        return true;
    };
    let Some(current_days) = parse_date_to_days(current.timestamp()) else {
        return true;
    };
    (current_days - last_days) >= interval.days as i64
}

/// Extract YYYYMMDD from a timestamp string and convert to an approximate day count.
fn parse_date_to_days(timestamp: &str) -> Option<i64> {
    // Timestamps are like "20230101", "20230101T1531", "20230101T153123+0200"
    // We only need the first 8 chars (YYYYMMDD)
    if timestamp.len() < 8 {
        return None;
    }
    let date_str = &timestamp[..8];
    let year: i64 = date_str[0..4].parse().ok()?;
    let month: i64 = date_str[4..6].parse().ok()?;
    let day: i64 = date_str[6..8].parse().ok()?;
    // Approximate day count (sufficient for interval comparison)
    Some(year * 365 + month * 30 + day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::BackendSpec;
    use crate::model::dest::{DestMeta, VolumeArchive};
    use crate::model::source::{Series, Timestamp};
    use crate::utils::duration::CalendarDuration;

    fn snap(name: &str) -> Timestamp {
        Timestamp::parse(name).unwrap()
    }

    fn src(names: &[&str]) -> SourceState {
        SourceState {
            volume: Series::new("test".to_string(), names.iter().map(|n| snap(n)).collect()),
        }
    }

    fn dest_empty() -> DestState {
        DestState { meta: None }
    }

    fn dest_with(archives: &[(&str, Option<&str>)]) -> DestState {
        DestState {
            meta: Some(DestMeta::new(
                0,
                0,
                archives
                    .iter()
                    .map(|(n, parent)| VolumeArchive {
                        timestamp: snap(n),
                        parent_timestamp: parent.map(|p| p.to_string()),
                        chunks: vec![],
                    })
                    .collect(),
            )),
        }
    }

    fn dest_with_full(names: &[&str]) -> DestState {
        dest_with(&names.iter().map(|n| (*n, None)).collect::<Vec<_>>())
    }

    fn dest_spec() -> DestSpec {
        DestSpec {
            backend_spec: BackendSpec::Local {
                path: "/tmp/test".to_string(),
            },
        }
    }

    /// Force all-full sends by setting max_incremental_depth to 1
    /// (every incremental candidate exceeds depth immediately).
    fn all_full_policy() -> SendPolicy {
        SendPolicy {
            min_full_send_interval: CalendarDuration { days: 7 },
            max_incremental_depth: Some(1),
        }
    }

    fn incremental_policy(interval_days: u32, max_depth: Option<u32>) -> SendPolicy {
        SendPolicy {
            min_full_send_interval: CalendarDuration {
                days: interval_days,
            },
            max_incremental_depth: max_depth,
        }
    }

    // --- Existing tests (backward compat with default policy) ---

    #[test]
    fn test_all_new_snapshots() {
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103"]);
        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest_empty(), &all_full_policy())
            .unwrap();

        assert_eq!(plan.steps.len(), 3);
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230101"));
        assert!(matches!(&plan.steps[1], RunStep::SendFull(s) if s.raw() == "20230102"));
        assert!(matches!(&plan.steps[2], RunStep::SendFull(s) if s.raw() == "20230103"));
    }

    #[test]
    fn test_some_already_archived() {
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103", "20230104"]);
        let dest = dest_with_full(&["20230101", "20230102"]);
        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &all_full_policy())
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230103"));
        assert!(matches!(&plan.steps[1], RunStep::SendFull(s) if s.raw() == "20230104"));
    }

    #[test]
    fn test_all_already_archived() {
        let planner = Planner;
        let src = src(&["20230101", "20230102"]);
        let dest = dest_with_full(&["20230101", "20230102"]);
        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &all_full_policy())
            .unwrap();

        assert!(plan.steps.is_empty());
    }

    #[test]
    fn test_single_snapshot() {
        let planner = Planner;
        let src = src(&["20230101"]);
        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest_empty(), &all_full_policy())
            .unwrap();

        assert_eq!(plan.steps.len(), 1);
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230101"));
    }

    // --- New incremental tests ---

    #[test]
    fn test_incremental_basic() {
        // dest has one full, new snapshots should be sent incrementally
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103"]);
        let dest = dest_with_full(&["20230101"]);
        let policy = incremental_policy(365, None); // large interval, won't trigger full

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert!(
            matches!(&plan.steps[0], RunStep::SendIncremental(s, p) if s.raw() == "20230102" && p.raw() == "20230101")
        );
        assert!(
            matches!(&plan.steps[1], RunStep::SendIncremental(s, p) if s.raw() == "20230103" && p.raw() == "20230102")
        );
    }

    #[test]
    fn test_max_depth_forces_full() {
        // max_depth=2: after 2 incrementals, next must be full
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103", "20230104"]);
        let dest = dest_with_full(&["20230101"]);
        let policy = incremental_policy(365, Some(2));

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 3);
        // depth 0->1 (ok)
        assert!(matches!(&plan.steps[0], RunStep::SendIncremental(s, _) if s.raw() == "20230102"));
        // depth 1->2 would be >= max_depth(2), so full
        assert!(matches!(&plan.steps[1], RunStep::SendFull(s) if s.raw() == "20230103"));
        // depth resets, 0->1 (ok)
        assert!(matches!(&plan.steps[2], RunStep::SendIncremental(s, _) if s.raw() == "20230104"));
    }

    #[test]
    fn test_interval_forces_full() {
        // interval of 30 days; snapshots 31 days apart should trigger full
        let planner = Planner;
        let src = src(&["20230101", "20230115", "20230201"]);
        let dest = dest_with_full(&["20230101"]);
        let policy = incremental_policy(30, None);

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        // 20230115 is 14 days after 20230101, no full needed
        assert!(matches!(&plan.steps[0], RunStep::SendIncremental(s, _) if s.raw() == "20230115"));
        // 20230201 is 31 days after 20230101 (last full), full needed
        assert!(matches!(&plan.steps[1], RunStep::SendFull(s) if s.raw() == "20230201"));
    }

    #[test]
    fn test_no_parent_on_source_forces_full() {
        // dest has a parent but source doesn't have it anymore
        let planner = Planner;
        // source only has 20230103 (20230101 and 20230102 deleted from source)
        let src = src(&["20230103"]);
        let dest = dest_with_full(&["20230101", "20230102"]);
        let policy = incremental_policy(365, None);

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 1);
        // No parent on source that also exists in dest → full
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230103"));
    }

    #[test]
    fn test_first_snapshot_always_full() {
        // Empty dest, even with incremental enabled, first must be full
        let planner = Planner;
        let src = src(&["20230101"]);
        let policy = incremental_policy(30, None);

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest_empty(), &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 1);
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230101"));
    }

    #[test]
    fn test_default_policy_sends_incremental() {
        // Default policy (1w interval) sends incrementally within the interval
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103"]);
        let dest = dest_with_full(&["20230101"]);
        let policy = incremental_policy(7, None); // default: 1w

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert!(
            matches!(&plan.steps[0], RunStep::SendIncremental(s, p) if s.raw() == "20230102" && p.raw() == "20230101")
        );
        assert!(
            matches!(&plan.steps[1], RunStep::SendIncremental(s, p) if s.raw() == "20230103" && p.raw() == "20230102")
        );
    }

    #[test]
    fn test_all_full_policy() {
        // max_incremental_depth=1 forces all sends to be full
        let planner = Planner;
        let src = src(&["20230101", "20230102", "20230103"]);
        let dest = dest_with_full(&["20230101"]);
        let policy = all_full_policy();

        let plan = planner
            .build_plan("vol", &src, &dest_spec(), &dest, &policy)
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert!(matches!(&plan.steps[0], RunStep::SendFull(s) if s.raw() == "20230102"));
        assert!(matches!(&plan.steps[1], RunStep::SendFull(s) if s.raw() == "20230103"));
    }
}

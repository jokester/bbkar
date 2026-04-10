mod prune;
mod restore;
mod send;
mod time;

use crate::DestSpec;
use crate::model::dest::{DestMeta, DestState, VolumeArchive};
use crate::model::error::BR;
use crate::model::plan::{PrunePlan, RestorePlan, RunPlan};
use crate::model::policy::{RetentionPolicy, SendPolicy};
use crate::model::source::Timestamp;
use crate::service::executor::inspect_source::SourceState;

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
        send::build_send_plan(volume, src_state, dest_spec, dest_state, send_policy)
    }

    /// Build a restore plan: walk the parent chain from `target` back to a full
    /// backup, then return steps in apply-order (full first, then incrementals).
    pub fn build_restore_plan(
        &self,
        archives: &[VolumeArchive],
        target: &Timestamp,
    ) -> BR<RestorePlan> {
        restore::build_restore_plan(archives, target)
    }

    pub fn build_prune_plan(
        &self,
        meta: Option<&DestMeta>,
        retention_policy: &RetentionPolicy,
    ) -> PrunePlan {
        prune::build_prune_plan(meta, retention_policy)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn build_prune_plan_at(
        &self,
        meta: Option<&DestMeta>,
        retention_policy: &RetentionPolicy,
        now_days: i64,
    ) -> PrunePlan {
        prune::build_prune_plan_at(meta, retention_policy, now_days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::BackendSpec;
    use crate::model::dest::{DestMeta, VolumeArchive};
    use crate::model::plan::{PruneReason, PruneStep, RunStep};
    use crate::model::source::{Series, Timestamp};
    use crate::service::planner::time::day_number_from_ymd;
    use crate::utils::duration::{CalendarDuration, Weekday};

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

    fn retention_policy_all() -> RetentionPolicy {
        RetentionPolicy {
            archive_preserve_min: None,
            archive_preserve: None,
            preserve_day_of_week: Weekday::Sunday,
        }
    }

    fn retention_policy_days(days: u32) -> RetentionPolicy {
        RetentionPolicy {
            archive_preserve_min: Some(CalendarDuration { days }),
            archive_preserve: None,
            preserve_day_of_week: Weekday::Sunday,
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

    #[test]
    fn test_prune_plan_commits_metadata_before_delete_steps() {
        let planner = Planner;
        let meta = DestMeta::new(
            1,
            2,
            vec![
                VolumeArchive {
                    timestamp: snap("20230101"),
                    parent_timestamp: None,
                    chunks: vec![],
                },
                VolumeArchive {
                    timestamp: snap("20230102"),
                    parent_timestamp: None,
                    chunks: vec![],
                },
            ],
        );
        let plan = planner.build_prune_plan_at(Some(&meta), &retention_policy_days(1), day_number_from_ymd(2023, 1, 10));

        assert!(matches!(plan.steps.first(), Some(PruneStep::CommitMetadata(_))));
        assert!(matches!(plan.steps.get(1), Some(PruneStep::DeleteArchive(a)) if a.timestamp.raw() == "20230101"));
        assert!(matches!(plan.steps.get(2), Some(PruneStep::DeleteArchive(a)) if a.timestamp.raw() == "20230102"));
    }

    #[test]
    fn test_prune_plan_keeps_unparseable_timestamps_conservatively() {
        let planner = Planner;
        let meta = DestMeta::new(
            1,
            2,
            vec![VolumeArchive {
                timestamp: snap("badstamp"),
                parent_timestamp: None,
                chunks: vec![],
            }],
        );
        let plan = planner.build_prune_plan_at(Some(&meta), &retention_policy_days(1), day_number_from_ymd(2023, 1, 10));

        assert_eq!(plan.pruned_count(), 0);
        assert!(matches!(plan.decisions[0].reason, PruneReason::KeepAll));
    }

    #[test]
    fn test_prune_plan_keep_all_produces_no_delete_steps() {
        let planner = Planner;
        let meta = DestMeta::new(
            1,
            2,
            vec![VolumeArchive {
                timestamp: snap("20230101"),
                parent_timestamp: None,
                chunks: vec![],
            }],
        );
        let plan = planner.build_prune_plan_at(Some(&meta), &retention_policy_all(), day_number_from_ymd(2023, 1, 10));

        assert!(plan.steps.is_empty());
        assert_eq!(plan.kept_count(), 1);
    }
}

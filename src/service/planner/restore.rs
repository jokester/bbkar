use crate::model::dest::VolumeArchive;
use crate::model::error::{BR, BbkarError};
use crate::model::plan::{RestorePlan, RestoreStep};
use crate::model::source::Timestamp;

pub(crate) fn build_restore_plan(archives: &[VolumeArchive], target: &Timestamp) -> BR<RestorePlan> {
    let mut chain = Vec::new();

    let target_archive = archives
        .iter()
        .find(|a| a.timestamp == *target)
        .ok_or_else(|| BbkarError::Plan(format!("snapshot '{}' not found in archives", target.raw())))?;

    chain.push(target_archive);

    let mut current = target_archive;
    while let Some(ref parent_raw) = current.parent_timestamp {
        current = archives
            .iter()
            .find(|a| a.timestamp.raw() == parent_raw.as_str())
            .ok_or_else(|| {
                BbkarError::Plan(format!(
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
                RestoreStep::ReceiveIncremental(a.timestamp.clone(), Timestamp::parse(parent).unwrap())
            } else {
                RestoreStep::ReceiveFull(a.timestamp.clone())
            }
        })
        .collect();

    Ok(RestorePlan { steps })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(name: &str) -> Timestamp {
        Timestamp::parse(name).unwrap()
    }

    fn archive(name: &str, parent: Option<&str>) -> VolumeArchive {
        VolumeArchive {
            timestamp: snap(name),
            parent_timestamp: parent.map(str::to_string),
            chunks: vec![],
        }
    }

    #[test]
    fn test_build_restore_plan_for_full_snapshot() {
        let archives = vec![archive("20230101", None)];
        let plan = build_restore_plan(&archives, &snap("20230101")).unwrap();

        assert_eq!(plan.steps.len(), 1);
        assert!(matches!(&plan.steps[0], RestoreStep::ReceiveFull(ts) if ts.raw() == "20230101"));
    }

    #[test]
    fn test_build_restore_plan_for_incremental_chain() {
        let archives = vec![
            archive("20230101", None),
            archive("20230102", Some("20230101")),
            archive("20230103", Some("20230102")),
        ];
        let plan = build_restore_plan(&archives, &snap("20230103")).unwrap();

        assert_eq!(plan.steps.len(), 3);
        assert!(matches!(&plan.steps[0], RestoreStep::ReceiveFull(ts) if ts.raw() == "20230101"));
        assert!(matches!(&plan.steps[1], RestoreStep::ReceiveIncremental(ts, parent) if ts.raw() == "20230102" && parent.raw() == "20230101"));
        assert!(matches!(&plan.steps[2], RestoreStep::ReceiveIncremental(ts, parent) if ts.raw() == "20230103" && parent.raw() == "20230102"));
    }

    #[test]
    fn test_build_restore_plan_errors_when_target_missing() {
        let archives = vec![archive("20230101", None)];
        let err = match build_restore_plan(&archives, &snap("20230102")) {
            Ok(_) => panic!("expected missing target error"),
            Err(err) => err,
        };

        assert!(matches!(err, BbkarError::Plan(msg) if msg.contains("snapshot '20230102' not found")));
    }

    #[test]
    fn test_build_restore_plan_errors_when_parent_missing() {
        let archives = vec![archive("20230102", Some("20230101"))];
        let err = match build_restore_plan(&archives, &snap("20230102")) {
            Ok(_) => panic!("expected missing parent error"),
            Err(err) => err,
        };

        assert!(matches!(err, BbkarError::Plan(msg) if msg.contains("parent snapshot '20230101' not found")));
    }
}

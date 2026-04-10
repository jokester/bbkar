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

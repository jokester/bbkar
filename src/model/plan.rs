use crate::model::dest::{DestMeta, VolumeArchive};
use crate::Timestamp;

pub struct RunPlan {
    pub steps: Vec<RunStep>,
}

#[derive(Debug)]
pub enum RunStep {
    SendFull(Timestamp),
    SendIncremental(Timestamp, Timestamp),
}

pub struct RestorePlan {
    pub steps: Vec<RestoreStep>,
}

#[derive(Debug)]
pub enum RestoreStep {
    ReceiveFull(Timestamp),
    ReceiveIncremental(Timestamp, Timestamp), // (snapshot, parent)
}

pub struct PrunePlan {
    pub decisions: Vec<PruneDecision>,
    pub steps: Vec<PruneStep>,
    pub resulting_meta: Option<DestMeta>,
}

impl PrunePlan {
    pub fn kept_count(&self) -> usize {
        self.decisions.iter().filter(|d| !d.would_prune()).count()
    }

    pub fn pruned_count(&self) -> usize {
        self.decisions.iter().filter(|d| d.would_prune()).count()
    }

    pub fn required_ancestor_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| matches!(d.reason, PruneReason::RequiredAncestor))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneStep {
    // Metadata is committed before data deletion so the visible archive set
    // changes atomically from the user's perspective even if file cleanup fails.
    CommitMetadata(DestMeta),
    DeleteArchive(VolumeArchive),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneDecision {
    pub snapshot: Timestamp,
    pub reason: PruneReason,
}

impl PruneDecision {
    pub fn would_prune(&self) -> bool {
        matches!(self.reason, PruneReason::PruneCandidate)
    }

    pub fn prune_status(&self) -> &'static str {
        match self.reason {
            PruneReason::PruneCandidate => "prune",
            PruneReason::RequiredAncestor => "keep(required)",
            _ => "keep",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneReason {
    KeepAll,
    TooNew,
    PreserveDay,
    PreserveWeek,
    PreserveMonth,
    PreserveYear,
    RequiredAncestor,
    PruneCandidate,
}

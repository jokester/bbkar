use crate::snapshot::Snapshot;

pub trait SnapshotManager {
    fn list_snapshots() -> Vec<Snapshot>;
}


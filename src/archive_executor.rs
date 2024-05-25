use crate::archive::Archive;
use crate::snapshot::Snapshot;

pub trait ArchiveExecutor {
    fn list_archive() -> Vec<Archive>;
    fn save_snapshot(s: Snapshot, parent: Option<Snapshot>);
    fn load_archive(a: Archive, dest: &str /* TODO: options */);
}

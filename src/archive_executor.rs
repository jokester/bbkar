use crate::archive::Archive;
use crate::snapshot::Snapshot;

pub trait ArchiveManager {
    fn list_archive() -> Vec<Archive>;
    fn backup_snapshot(s: Snapshot);
    fn restore_archive(a: Archive, dest: &str);
}

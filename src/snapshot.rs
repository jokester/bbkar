use regex::Regex;
use std::path::PathBuf;

pub struct Snapshot {
    pub _snapshots_dir: String,
    pub volume: String,
    pub timestamp: SnapshotTimestamp,
}

const subvol_pattern: Regex = Regex::new(r"^()").unwrap();

impl Snapshot {
    pub fn from_pathbuf(buffer: &PathBuf) -> Option<Snapshot> {
        todo!()
    }
    pub fn from_string() -> Option<Snapshot> {
        todo!()
    }
}

pub struct SnapshotTimestamp {
    timestamp: String,
    step: Option<u8>,
}

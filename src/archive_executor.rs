use crate::archive::Archive;
use crate::snapshot::Snapshot;
use opendal::Scheme;
use opendal::{Operator, Result as R};
use std::collections::HashMap;
use std::path::PathBuf;

struct ArchiveManager {}

impl ArchiveManager {
    fn build_operator(scheme_name: &str, args: HashMap<String, String>) -> R<Operator> {
        let scheme = scheme_name.parse::<Scheme>()?;
        let op = Operator::via_map(scheme, args)?;
        Ok(op)
    }
}

pub trait ArchiveExecutor {
    fn list_archive() -> R<Vec<Archive>>;
    fn save_snapshot(s: Snapshot, parent: Option<Snapshot>) -> R<()>;
    fn load_archive(a: Archive, dest: &PathBuf /* TODO: options */);
}

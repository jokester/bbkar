use crate::snapshot::{Snapshot, SnapshotTimestamp};
use std::path::{Path, PathBuf};
use std::{fs, io};
use subprocess::{Exec, Result as SResult};

pub struct SnapshotConfig {
    snapshot_dir: Path,
}

impl SnapshotConfig {
    fn list_snapshot(&self) -> io::Result<Vec<Snapshot>> {
        // let c = fs::read_dir(&self.snapshot_dir)?.map(Snapshot::from_string);
        // let children =  self.snapshot_dir.read_dir()?.map(Snapshot::from_string).collect::<Result<Vec<_>, io::Error>>();
        let entries: Vec<PathBuf> = fs::read_dir(&self.snapshot_dir)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;
        let entries: Vec<Snapshot> = entries
            .iter()
            .filter_map(|pathbuf| Snapshot::from_pathbuf(pathbuf))
            .collect();
        return Ok(entries.clone()).cloned();
    }
    fn read_snapshot(&self, s: Snapshot, parent: Option<Snapshot>) {
        todo!()
    }
}

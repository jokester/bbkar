use crate::snapshot::{Snapshot};
use std::path::{PathBuf};
use std::{fs, io};

pub struct SnapshotConfig {
    pub snapshot_dir: PathBuf,
}

impl SnapshotConfig {
    pub fn list_snapshot(&self) -> io::Result<Vec<Snapshot>> {
        // let c = fs::read_dir(&self.snapshot_dir)?.map(Snapshot::from_string);
        // let children =  self.snapshot_dir.read_dir()?.map(Snapshot::from_string).collect::<Result<Vec<_>, io::Error>>();
        let entries: Vec<PathBuf> = fs::read_dir(&self.snapshot_dir.as_path())?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;
        let entries: Vec<Snapshot> = entries
            .iter()
            .filter_map(|pathbuf| Snapshot::from_pathbuf(pathbuf))
            .collect();
        return Ok(entries);
    }
    fn read_snapshot(&self, s: Snapshot, parent: Option<Snapshot>) {
        todo!()
    }
}

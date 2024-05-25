use crate::snapshot::{Snapshot, SnapshotTimestamp};
use std::path::Path;
use std::{fs, io};
use subprocess::{Exec, Result as SResult};

pub trait SnapshotExecutor {
    fn list_snapshot(&self ) -> SResult<Vec<Snapshot>>;
    // TODO: what is a pipeable stream type?
    fn read_snapshot(&self, s: Snapshot, parent: Option<Snapshot>);
}

pub struct SnapshotConfig {
    snapshot_dir: Path
}

impl SnapshotConfig {
    fn wtf(self: &SnapshotConfig) {
        let mut x: Vec<String> = Vec::new();
        let args = vec!["subvolume", "list", self.snapshot_dir.as_str()];
        let exit_status = subprocess::Exec::cmd("btrfs").args(args.as_slice()).join()?;
    }
}

impl SnapshotExecutor for SnapshotConfig {
    fn list_snapshot(&self) -> io::Result<Vec<Snapshot>> {
        // let c = fs::read_dir(&self.snapshot_dir)?.map(Snapshot::from_string);
        // let children =  self.snapshot_dir.read_dir()?.map(Snapshot::from_string).collect::<Result<Vec<_>, io::Error>>();
        let entries = fs::read_dir(&self.snapshot_dir)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()
            .map(Snapshot::from_pathbuf).collect()
            ;
        return entries;
    }
    fn read_snapshot(&self, s: Snapshot, parent: Option<Snapshot>) {
        todo!()
    }
}

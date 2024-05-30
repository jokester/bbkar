use bbkar::snapshot_executor::SnapshotConfig;
use std::path::{PathBuf};
fn main() {
    let conf = SnapshotConfig {
        snapshot_dir: (PathBuf::from("/media/data-root/_btrbk_snap")),
    };
    let snapshots = conf.list_snapshot().unwrap();

    snapshots.iter().for_each(|snapshot| {
        println!("snapshot: {:?}", snapshot);
    });
}

use bbkar::error::{BbkarError, BR};
use bbkar::snapshot_executor::SnapshotConfig;
use io::{read_to_string, Result as IR};
use std::any::Any;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use toml::Table;

fn demo_list_snapshots() {
    let conf = SnapshotConfig {
        snapshot_dir: PathBuf::from("/media/data-root/_btrbk_snap"),
    };
    let snapshots = conf.list_snapshot().unwrap();

    snapshots.iter().for_each(|snapshot| {
        println!("snapshot: {:?}", snapshot);
    });
}

fn demo_read_config() -> IR<String> {
    let conf_path = "examples/gcs.toml";
    fs::read_to_string(conf_path)
}
fn main() -> BR<()> {
    let conf_str = demo_read_config()?;
    print!("conf_str = {}", conf_str);

    let conf_map = conf_str.parse::<Table>().unwrap();
    print!("conf_map = {}", conf_map);
    Ok(())
}

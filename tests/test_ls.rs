mod common;

use std::collections::HashMap;

use bbkar::model::dest::{DestMeta, DestState, VolumeArchive};
use bbkar::model::source::{Series, Timestamp};
use bbkar::service::executor::inspect_source::SourceState;

fn snap(name: &str) -> Timestamp {
    Timestamp::parse(name).unwrap()
}

fn source_state(volume_name: &str, names: &[&str]) -> SourceState {
    SourceState {
        volume: Series::new(
            volume_name.to_string(),
            names.iter().map(|n| snap(n)).collect(),
        ),
    }
}

fn dest_with(names: &[&str]) -> DestState {
    DestState {
        meta: Some(DestMeta::new(
            1000,
            2000,
            names
                .iter()
                .map(|n| VolumeArchive {
                    timestamp: snap(n),
                    parent_timestamp: None,
                    chunks: vec![],
                })
                .collect(),
        )),
    }
}

#[test]
fn test_ls_marks_local_remote_and_synced_snapshots() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230104"]),
    );

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        dest_with(&["20230101", "20230103", "20230104"]),
    );

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::ls(&config_path, None, None, Box::new(executor)).unwrap();

    let lines = output.lines();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("snapshot") && l.contains("state"))
    );

    let idx_01 = lines
        .iter()
        .position(|l| l.contains("20230101"))
        .expect("missing synced snapshot");
    let idx_02 = lines
        .iter()
        .position(|l| l.contains("20230102") && l.contains("local-only"))
        .expect("missing local-only snapshot");
    let idx_03 = lines
        .iter()
        .position(|l| l.contains("20230103") && l.contains("remote-only"))
        .expect("missing remote-only snapshot");
    let idx_04 = lines
        .iter()
        .position(|l| l.contains("20230104"))
        .expect("missing synced snapshot");

    assert!(idx_01 < idx_02 && idx_02 < idx_03 && idx_03 < idx_04);

    let synced_01 = &lines[idx_01];
    assert!(synced_01.contains("20230101"));
    assert!(synced_01.contains("synced"));

    let synced_04 = &lines[idx_04];
    assert!(synced_04.contains("20230104"));
    assert!(synced_04.contains("synced"));
}

#[test]
fn test_ls_handles_empty_destination() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert("myvol".to_string(), source_state("myvol", &["20230101"]));

    let dests = HashMap::new();

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::ls(&config_path, None, None, Box::new(executor)).unwrap();

    assert!(
        output
            .lines()
            .iter()
            .any(|l| l.contains("20230101") && l.contains("local-only"))
    );
}

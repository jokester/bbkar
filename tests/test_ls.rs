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
            .any(|l| l.contains("snapshot") && l.contains("state") && l.contains("prune"))
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
    assert!(synced_01.contains("keep"));

    let synced_04 = &lines[idx_04];
    assert!(synced_04.contains("20230104"));
    assert!(synced_04.contains("synced"));

    let local_only = &lines[idx_02];
    assert!(local_only.contains("-"));

    let remote_only = &lines[idx_03];
    assert!(remote_only.contains("keep"));
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
            .any(|l| l.contains("20230101") && l.contains("local-only") && l.contains("-"))
    );
}

#[test]
fn test_ls_marks_remote_archive_for_prune() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert("myvol".to_string(), source_state("myvol", &["20990110"]));

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        dest_with(&["20230101", "20230110"]),
    );

    let config_path = common::write_config_file(
        &tmp,
        r#"[global]

[source.src1]
path = "/fake/source"

[dest.dst1]
driver = "local"
path = "/fake/dest"

[sync.main]
source = "src1"
dest = "dst1"
filter = ["myvol"]
archive_preserve_min = "1d"
"#,
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::ls(&config_path, None, None, Box::new(executor)).unwrap();

    assert!(
        output
            .lines()
            .iter()
            .any(|l| l.contains("20230101") && l.contains("remote-only") && l.contains("prune")),
        "expected old remote archive to be marked for prune, got:\n{}",
        output.text()
    );
}

#[test]
fn test_ls_marks_required_ancestor_archive() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert("myvol".to_string(), source_state("myvol", &["20990110"]));

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        DestState {
            meta: Some(DestMeta::new(
                1000,
                2000,
                vec![
                    VolumeArchive {
                        timestamp: snap("20230101"),
                        parent_timestamp: None,
                        chunks: vec![],
                    },
                    VolumeArchive {
                        timestamp: snap("20990110"),
                        parent_timestamp: Some("20230101".to_string()),
                        chunks: vec![],
                    },
                ],
            )),
        },
    );

    let config_path = common::write_config_file(
        &tmp,
        r#"[global]

[source.src1]
path = "/fake/source"

[dest.dst1]
driver = "local"
path = "/fake/dest"

[sync.main]
source = "src1"
dest = "dst1"
filter = ["myvol"]
archive_preserve_min = "1d"
"#,
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::ls(&config_path, None, None, Box::new(executor)).unwrap();

    assert!(
        output
            .lines()
            .iter()
            .any(|l| l.contains("20230101") && l.contains("remote-only") && l.contains("keep(required)")),
        "expected ancestor archive to be kept as required, got:\n{}",
        output.text()
    );
    assert!(
        output
            .lines()
            .iter()
            .any(|l| l.contains("20990110") && l.contains("synced") && l.contains("keep")),
        "expected child archive to be kept, got:\n{}",
        output.text()
    );
}

#[test]
fn test_ls_restore_root_mode_keeps_restore_table_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let restore_root = tmp.path().join("restore-root");
    std::fs::create_dir_all(&restore_root).unwrap();
    std::fs::write(restore_root.join("myvol.20230101"), "").unwrap();
    std::fs::write(restore_root.join("myvol.20230103"), "").unwrap();

    let mut sources = HashMap::new();
    sources.insert("myvol".to_string(), source_state("myvol", &["20230101"]));

    let mut dests = HashMap::new();
    dests.insert("myvol".to_string(), dest_with(&["20230101", "20230102"]));

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::ls(
        &config_path,
        None,
        Some(restore_root.to_str().unwrap()),
        Box::new(executor),
    )
    .unwrap();

    let text = output.text();
    assert!(text.contains("snapshot"));
    assert!(text.contains("state"));
    assert!(!text.contains("prune"));
    assert!(text.contains("20230101") && text.contains("restored"));
    assert!(text.contains("20230102") && text.contains("not-restored"));
    assert!(text.contains("20230103") && text.contains("restored-only"));
}

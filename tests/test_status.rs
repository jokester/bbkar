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
fn test_status_basic() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

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
    bbkar::cli::status(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();

    // Should show 3 local snapshots with range
    assert!(
        text.contains("3 local snapshots"),
        "expected '3 local snapshots' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("20230101") && text.contains("20230103"),
        "expected snapshot range in output, got:\n{}",
        text
    );

    // Should show remote range and the shared archive stats block
    assert!(
        text.contains("remote range (see `bbkar ls` for details): 20230101 - 20230102"),
        "expected remote range in output, got:\n{}",
        text
    );
    assert!(
        text.contains("remote usage:"),
        "expected remote usage summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("remote usage: 2 archives, 0 bytes (full: 2 archives, 0 bytes; incremental: 0 archives, 0 bytes)"),
        "expected aggregate remote archive stats in output, got:\n{}",
        text
    );
    assert!(
        text.contains("send policy: full at least every 1w, no incremental depth limit"),
        "expected default send policy in output, got:\n{}",
        text
    );
    assert!(
        text.contains("retention: keep all archives"),
        "expected default retention policy in output, got:\n{}",
        text
    );
    assert!(
        text.contains("next prune: keep 2 archive(s), prune 0 archive(s), required ancestor 0 archive(s)"),
        "expected next prune summary in output, got:\n{}",
        text
    );
}

#[test]
fn test_status_empty_dest() {
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
    bbkar::cli::status(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();

    // Should show 1 local snapshot
    assert!(
        text.contains("1 local snapshots"),
        "expected '1 local snapshots' in output, got:\n{}",
        text
    );

    // Should show empty remote usage block
    assert!(
        text.contains("remote range (see `bbkar ls` for details): None - None"),
        "expected empty remote range in output, got:\n{}",
        text
    );
    assert!(
        text.contains("remote usage:"),
        "expected remote usage summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("remote usage: 0 archives, 0 bytes (full: 0 archives, 0 bytes; incremental: 0 archives, 0 bytes)"),
        "expected zero remote archive stats in output, got:\n{}",
        text
    );
    assert!(
        text.contains("next prune: keep 0 archive(s), prune 0 archive(s), required ancestor 0 archive(s)"),
        "expected empty next prune summary in output, got:\n{}",
        text
    );
}

#[test]
fn test_status_prints_non_default_policies() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

    let mut dests = HashMap::new();
    dests.insert("myvol".to_string(), dest_with(&["20230101", "20230102"]));

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
min_full_send_interval = "30d"
max_incremental_depth = 5
archive_preserve_min = "7d"
archive_preserve = "30d 12w 6m *y"
preserve_day_of_week = "monday"
"#,
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 0);
    bbkar::cli::status(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        text.contains("send policy: full at least every 1m, max incremental depth 5"),
        "expected custom send policy in output, got:\n{}",
        text
    );
    assert!(
        text.contains("retention: keep all archives for 1w, then preserve 30d 12w 6m *y (week anchor: monday)"),
        "expected custom retention policy in output, got:\n{}",
        text
    );
}

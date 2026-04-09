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
}

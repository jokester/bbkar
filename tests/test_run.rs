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
            0,
            0,
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
fn test_run_up_to_date() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102"]),
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
    bbkar::cli::run(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        text.contains("(up to date)"),
        "expected '(up to date)' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent:"),
        "expected sync sent summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent: 0 archives, 0 bytes (full: 0 archives, 0 bytes; incremental: 0 archives, 0 bytes)"),
        "expected zero archive summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync remote usage:"),
        "expected sync remote usage summary in output, got:\n{}",
        text
    );
}

#[test]
fn test_run_new_volume_sends_full() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102"]),
    );

    let dests = HashMap::new();

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 1024);
    bbkar::cli::run(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        text.contains("sending full"),
        "expected 'sending full' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("done: 20230101 (sent 1.00 KiB compressed, 2.00 KiB raw)"),
        "expected 'done: 20230101' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("done: 20230102 (sent 1.00 KiB compressed, 2.00 KiB raw)"),
        "expected 'done: 20230102' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent:"),
        "expected sync sent summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent: 2 archives, 2.00 KiB (full: 1 archives, 1.00 KiB; incremental: 1 archives, 1.00 KiB)"),
        "expected aggregate archive stats in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync remote usage:"),
        "expected sync remote usage summary in output, got:\n{}",
        text
    );
}

#[test]
fn test_run_partially_synced_sends_incremental() {
    let tmp = tempfile::tempdir().unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

    let mut dests = HashMap::new();
    dests.insert("myvol".to_string(), dest_with(&["20230101"]));

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output) = common::MockExecutor::new(sources, dests, 512);
    bbkar::cli::run(&config_path, None, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        !text.contains("(up to date)"),
        "should not be up to date, got:\n{}",
        text
    );
    assert!(
        text.contains("sending incremental") || text.contains("sending full"),
        "expected send operations in output, got:\n{}",
        text
    );
    assert!(
        text.contains("done: 20230102 (sent 512 bytes compressed, 1.00 KiB raw)"),
        "expected 'done: 20230102' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("done: 20230103 (sent 512 bytes compressed, 1.00 KiB raw)"),
        "expected 'done: 20230103' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent:"),
        "expected sync sent summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync sent: 2 archives, 1.00 KiB (full: 0 archives, 0 bytes; incremental: 2 archives, 1.00 KiB)"),
        "expected aggregate archive stats in output, got:\n{}",
        text
    );
}

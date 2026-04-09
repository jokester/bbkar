mod common;

use std::collections::HashMap;

use bbkar::cli::Commands;
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
fn test_dryrun_up_to_date() {
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
    let cmd = Commands::Dryrun {
        measure_send: true,
        name: None,
    };
    bbkar::cli::dryrun(&config_path, cmd, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        text.contains("(up to date)"),
        "expected '(up to date)' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync would send:"),
        "expected sync would-send summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync would send: 0 archives, 0 bytes (full: 0 archives, 0 bytes; incremental: 0 archives, 0 bytes)"),
        "expected zero archive stats in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync remote usage:"),
        "expected sync remote usage summary in output, got:\n{}",
        text
    );
}

#[test]
fn test_dryrun_new_volume_full_send() {
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
    let cmd = Commands::Dryrun {
        measure_send: true,
        name: None,
    };
    bbkar::cli::dryrun(&config_path, cmd, Box::new(executor)).unwrap();

    let text = output.text();
    assert!(
        text.contains("would send full"),
        "expected 'would send full' in output, got:\n{}",
        text
    );
    // Both snapshots should appear as full sends
    assert!(
        text.contains("20230101"),
        "expected '20230101' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("20230102"),
        "expected '20230102' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync would send:"),
        "expected sync would-send summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync would send: 2 archives, 2.00 KiB (full: 1 archives, 1.00 KiB; incremental: 1 archives, 1.00 KiB)"),
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
fn test_dryrun_partially_synced() {
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

    let (executor, output) = common::MockExecutor::new(sources, dests, 2048);
    let cmd = Commands::Dryrun {
        measure_send: true,
        name: None,
    };
    bbkar::cli::dryrun(&config_path, cmd, Box::new(executor)).unwrap();

    let lines = output.lines();
    // Should NOT contain "up to date"
    let text = output.text();
    assert!(
        !text.contains("(up to date)"),
        "should not be up to date, got:\n{}",
        text
    );
    // Missing snapshots should appear
    assert!(
        text.contains("20230102"),
        "expected '20230102' in output, got:\n{}",
        text
    );
    assert!(
        text.contains("20230103"),
        "expected '20230103' in output, got:\n{}",
        text
    );
    // Already-synced snapshot should NOT appear in "would send" lines
    let send_lines: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("would send full:") || l.contains("would send incremental:"))
        .collect();
    assert_eq!(
        send_lines.len(),
        2,
        "expected 2 'would send' lines, got: {:?}",
        send_lines
    );
    assert!(
        text.contains("sync would send:"),
        "expected sync would-send summary in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync would send: 2 archives, 4.00 KiB (full: 0 archives, 0 bytes; incremental: 2 archives, 4.00 KiB)"),
        "expected aggregate archive stats in output, got:\n{}",
        text
    );
    assert!(
        text.contains("sync remote usage:"),
        "expected sync remote usage summary in output, got:\n{}",
        text
    );
}

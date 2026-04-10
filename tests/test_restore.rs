mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bbkar::model::BtrfsSendChunk;
use bbkar::model::config::{DestSpec, SourceSpec};
use bbkar::model::dest::ChunkFilename;
use bbkar::model::dest::{DestMeta, DestState, VolumeArchive};
use bbkar::model::error::BR;
use bbkar::model::source::{Series, Timestamp};
use bbkar::service::executor::Executor;
use bbkar::service::executor::inspect_source::SourceState;
use bbkar::service::executor::measure::MeasureResult;
use bbkar::service::executor::write_dest::TransferStats;

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

fn archive(name: &str, parent: Option<&str>) -> VolumeArchive {
    VolumeArchive {
        timestamp: snap(name),
        parent_timestamp: parent.map(str::to_string),
        chunks: vec![],
    }
}

struct RestoreExecutor {
    sources: HashMap<String, SourceState>,
    dest_states: HashMap<String, DestState>,
    printed: Arc<Mutex<Vec<String>>>,
    restored: Arc<Mutex<Vec<String>>>,
}

impl RestoreExecutor {
    fn new(
        sources: HashMap<String, SourceState>,
        dest_states: HashMap<String, DestState>,
    ) -> (Self, common::PrintedOutput, Arc<Mutex<Vec<String>>>) {
        let printed = Arc::new(Mutex::new(Vec::new()));
        let restored = Arc::new(Mutex::new(Vec::new()));
        let output = common::PrintedOutput::from_shared(printed.clone());
        (
            Self {
                sources,
                dest_states,
                printed,
                restored: restored.clone(),
            },
            output,
            restored,
        )
    }
}

impl Executor for RestoreExecutor {
    fn inspect_source(&self, _src: &SourceSpec) -> BR<HashMap<String, SourceState>> {
        Ok(self.sources.clone())
    }

    fn inspect_dest_volume(&self, _spec: &DestSpec, volume: &str) -> BR<DestState> {
        Ok(self
            .dest_states
            .get(volume)
            .cloned()
            .unwrap_or(DestState { meta: None }))
    }

    fn read_subvolume_full(
        &self,
        _src: SourceSpec,
        _subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> {
        Box::new(std::iter::empty())
    }

    fn read_subvolume_incremental(
        &self,
        _src: SourceSpec,
        _subvolume: &str,
        _parent_subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> {
        Box::new(std::iter::empty())
    }

    fn write_metadata(
        &self,
        _dest_spec: &DestSpec,
        _volume_basename: &str,
        _new_meta: &bbkar::model::dest::DestMeta,
    ) -> BR<()> {
        Ok(())
    }

    fn write_subvolume(
        &self,
        _dest_spec: &DestSpec,
        _volume_basename: &str,
        _snapshot: &str,
        _max_chunk_size_mib: u64,
        _chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<(Vec<ChunkFilename>, TransferStats)> {
        unreachable!("write_subvolume is not used in restore tests")
    }

    fn measure_subvolume(
        &self,
        _chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<MeasureResult> {
        unreachable!("measure_subvolume is not used in restore tests")
    }

    fn restore_archive(
        &self,
        _dest_spec: &DestSpec,
        volume: &str,
        archive: &VolumeArchive,
        _receive_root: &str,
    ) -> BR<()> {
        self.restored
            .lock()
            .unwrap()
            .push(format!("{volume}.{}", archive.timestamp.raw()));
        Ok(())
    }

    fn print_info(&self, info: &str) {
        self.printed.lock().unwrap().push(info.to_string());
    }

    fn print_warn(&self, warn: &str) {
        self.printed
            .lock()
            .unwrap()
            .push(format!("Warning: {}", warn));
    }
}

#[test]
fn test_dryrestore_prints_chain_and_skips_existing_subvolumes() {
    let tmp = tempfile::tempdir().unwrap();
    let restore_root = tmp.path().join("restore-root");
    std::fs::create_dir_all(&restore_root).unwrap();
    std::fs::write(restore_root.join("myvol.20230102"), "").unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        DestState {
            meta: Some(DestMeta::new(
                1000,
                2000,
                vec![
                    archive("20230101", None),
                    archive("20230102", Some("20230101")),
                    archive("20230103", Some("20230102")),
                ],
            )),
        },
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
    bbkar::cli::dryrestore(
        &config_path,
        restore_root.to_str().unwrap(),
        "myvol",
        &["20230103".to_string()],
        None,
        None,
        None,
        Box::new(executor),
    )
    .unwrap();

    let text = output.text();
    assert!(text.contains("bbkar dryrestore"));
    assert!(text.contains("target timestamp: 20230103"));
    assert!(text.contains("restore chain: 3 step(s)"));
    assert!(text.contains("20230101 (full)"));
    assert!(text.contains("20230102 (incremental, parent: 20230101)"));
    assert!(text.contains("20230103 (incremental, parent: 20230102)"));
    assert!(text.contains("would receive full 20230101"));
    assert!(text.contains("would skip myvol.20230102 (already exists in root)"));
    assert!(text.contains("would receive incremental 20230103"));
    assert!(text.contains("dryrestore complete"));
}

#[test]
fn test_dryrestore_defaults_to_latest_target() {
    let tmp = tempfile::tempdir().unwrap();
    let restore_root = tmp.path().join("restore-root");
    std::fs::create_dir_all(&restore_root).unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102"]),
    );

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        DestState {
            meta: Some(DestMeta::new(
                1000,
                2000,
                vec![archive("20230101", None), archive("20230102", Some("20230101"))],
            )),
        },
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
    bbkar::cli::dryrestore(
        &config_path,
        restore_root.to_str().unwrap(),
        "myvol",
        &[],
        None,
        None,
        None,
        Box::new(executor),
    )
    .unwrap();

    assert!(output.text().contains("target timestamp: 20230102"));
}

#[test]
fn test_restore_replays_chain_and_skips_preexisting() {
    let tmp = tempfile::tempdir().unwrap();
    let restore_root = tmp.path().join("restore-root");
    std::fs::create_dir_all(&restore_root).unwrap();
    std::fs::write(restore_root.join("myvol.20230102"), "").unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        DestState {
            meta: Some(DestMeta::new(
                1000,
                2000,
                vec![
                    archive("20230101", None),
                    archive("20230102", Some("20230101")),
                    archive("20230103", Some("20230102")),
                ],
            )),
        },
    );

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, output, restored) = RestoreExecutor::new(sources, dests);
    bbkar::cli::restore(
        &config_path,
        restore_root.to_str().unwrap(),
        "myvol",
        &["20230103".to_string()],
        None,
        None,
        None,
        Box::new(executor),
    )
    .unwrap();

    let restored = restored.lock().unwrap().clone();
    assert_eq!(
        restored,
        vec!["myvol.20230101".to_string(), "myvol.20230103".to_string()]
    );

    let text = output.text();
    assert!(text.contains("receiving full 20230101"));
    assert!(text.contains("Warning:   skipping myvol.20230102 (already exists in root before restore, assuming valid)"));
    assert!(text.contains("receiving incremental 20230103"));
    assert!(text.contains("restore complete"));
}

#[test]
fn test_restore_deduplicates_steps_for_multiple_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let restore_root = tmp.path().join("restore-root");
    std::fs::create_dir_all(&restore_root).unwrap();

    let mut sources = HashMap::new();
    sources.insert(
        "myvol".to_string(),
        source_state("myvol", &["20230101", "20230102", "20230103"]),
    );

    let mut dests = HashMap::new();
    dests.insert(
        "myvol".to_string(),
        DestState {
            meta: Some(DestMeta::new(
                1000,
                2000,
                vec![
                    archive("20230101", None),
                    archive("20230102", Some("20230101")),
                    archive("20230103", Some("20230102")),
                ],
            )),
        },
    );

    let config_path = common::make_config_file(
        &tmp,
        "src1",
        "/fake/source",
        "dst1",
        "/fake/dest",
        &["myvol"],
    );

    let (executor, _output, restored) = RestoreExecutor::new(sources, dests);
    bbkar::cli::restore(
        &config_path,
        restore_root.to_str().unwrap(),
        "myvol",
        &["20230102".to_string(), "20230103".to_string()],
        None,
        None,
        None,
        Box::new(executor),
    )
    .unwrap();

    let restored = restored.lock().unwrap().clone();
    assert_eq!(
        restored,
        vec![
            "myvol.20230101".to_string(),
            "myvol.20230102".to_string(),
            "myvol.20230103".to_string()
        ]
    );
}

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bbkar::model::BtrfsSendChunk;
use bbkar::model::config::{DestSpec, SourceSpec};
use bbkar::model::dest::{ChunkFilename, DestState, VolumeArchive};
use bbkar::model::error::{BR, BbkarError};
use bbkar::service::executor::Executor;
use bbkar::service::executor::inspect_source::SourceState;
use bbkar::service::executor::measure::MeasureResult;
use bbkar::service::executor::write_dest::TransferStats;

/// A handle to inspect output captured by a MockExecutor after it has been consumed.
#[derive(Clone)]
pub struct PrintedOutput(Arc<Mutex<Vec<String>>>);

impl PrintedOutput {
    #[allow(dead_code)]
    pub fn from_shared(lines: Arc<Mutex<Vec<String>>>) -> Self {
        Self(lines)
    }

    pub fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn text(&self) -> String {
        self.lines().join("\n")
    }
}

pub struct MockExecutor {
    sources: HashMap<String, SourceState>,
    dest_states: HashMap<String, DestState>,
    measure_result: u64,
    printed: Arc<Mutex<Vec<String>>>,
}

impl MockExecutor {
    pub fn new(
        sources: HashMap<String, SourceState>,
        dest_states: HashMap<String, DestState>,
        measure_result: u64,
    ) -> (Self, PrintedOutput) {
        let printed = Arc::new(Mutex::new(Vec::new()));
        let handle = PrintedOutput(printed.clone());
        (
            Self {
                sources,
                dest_states,
                measure_result,
                printed,
            },
            handle,
        )
    }
}

impl Executor for MockExecutor {
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
        Box::new(vec![Ok(BtrfsSendChunk::ProcessExit(0, String::new()))].into_iter())
    }

    fn read_subvolume_incremental(
        &self,
        _src: SourceSpec,
        _subvolume: &str,
        _parent_subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> {
        Box::new(vec![Ok(BtrfsSendChunk::ProcessExit(0, String::new()))].into_iter())
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
        let chunks = vec![ChunkFilename::new(
            "part000001.btrfs.zstd".to_string(),
            self.measure_result as u32,
            Some("zstd".to_string()),
            Some(self.measure_result * 2),
            Some("deadbeef".to_string()),
        )];
        let stats = TransferStats {
            elapsed: std::time::Duration::from_secs(1),
            raw_bytes: self.measure_result * 2,
            compressed_bytes: self.measure_result,
            read_time: std::time::Duration::from_millis(400),
            write_time: std::time::Duration::from_millis(400),
        };
        Ok((chunks, stats))
    }

    fn measure_subvolume(
        &self,
        _chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<MeasureResult> {
        Ok(MeasureResult {
            uncompressed: self.measure_result * 2,
            compressed: self.measure_result,
        })
    }

    fn restore_archive(
        &self,
        _dest_spec: &DestSpec,
        _volume: &str,
        _archive: &VolumeArchive,
        _receive_root: &str,
    ) -> BR<()> {
        Err(BbkarError::Execution(
            "restore not supported in mock".to_string(),
        ))
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

    fn print_error(&self, error: &str) {
        self.printed
            .lock()
            .unwrap()
            .push(format!("Error: {}", error));
    }
}

/// Write a minimal valid config TOML and return its path.
pub fn make_config_file(
    dir: &tempfile::TempDir,
    source_name: &str,
    source_path: &str,
    dest_name: &str,
    dest_path: &str,
    filter: &[&str],
) -> PathBuf {
    let filter_toml: Vec<String> = filter.iter().map(|f| format!("'{}'", f)).collect();
    let content = format!(
        r#"[global]

[source.{source_name}]
path = "{source_path}"

[dest.{dest_name}]
driver = "local"
path = "{dest_path}"

[sync.main]
source = "{source_name}"
dest = "{dest_name}"
filter = [{filter}]
"#,
        source_name = source_name,
        source_path = source_path,
        dest_name = dest_name,
        dest_path = dest_path,
        filter = filter_toml.join(", "),
    );
    let path = dir.path().join("config.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

#[allow(dead_code)]
pub fn write_config_file(dir: &tempfile::TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("config.toml");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

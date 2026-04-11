use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use bbkar::model::BtrfsSendChunk;
use bbkar::model::config::{BackendSpec, DestSpec, SourceSpec};
use bbkar::model::dest::{DestMeta, DestState, VolumeArchive};
use bbkar::model::error::{BR, BbkarError};
use bbkar::model::source::Timestamp;
use bbkar::service::executor::inspect_source::SourceState;
use bbkar::service::executor::measure::MeasureResult;
use bbkar::service::executor::write_dest::TransferStats;
use bbkar::service::executor::{Executor, RealExecutor, summon_real_executor};
use tracing_subscriber::fmt::MakeWriter;

fn snap(name: &str) -> Timestamp {
    Timestamp::parse(name).unwrap()
}

fn local_dest(path: &std::path::Path) -> DestSpec {
    DestSpec {
        backend_spec: BackendSpec::Local {
            path: path.to_string_lossy().to_string(),
        },
    }
}

#[derive(Clone)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl<'a> MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard(self.0.clone())
    }
}

struct SharedWriterGuard(Arc<Mutex<Vec<u8>>>);

impl io::Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn capture_logs(emit: impl FnOnce()) -> String {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(SharedWriter(buffer.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, emit);

    String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
}

struct DummyExecutor;

impl Executor for DummyExecutor {
    fn inspect_source(&self, _src: &SourceSpec) -> BR<HashMap<String, SourceState>> {
        Ok(HashMap::new())
    }

    fn inspect_dest_volume(&self, _spec: &DestSpec, _volume: &str) -> BR<DestState> {
        Ok(DestState { meta: None })
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
        _new_meta: &DestMeta,
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
    ) -> BR<(Vec<bbkar::model::dest::ChunkFilename>, TransferStats)> {
        Ok((
            Vec::new(),
            TransferStats {
                elapsed: std::time::Duration::default(),
                raw_bytes: 0,
                compressed_bytes: 0,
                read_time: std::time::Duration::default(),
                write_time: std::time::Duration::default(),
            },
        ))
    }

    fn measure_subvolume(
        &self,
        _chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<MeasureResult> {
        Ok(MeasureResult {
            uncompressed: 0,
            compressed: 0,
        })
    }

    fn restore_archive(
        &self,
        _dest_spec: &DestSpec,
        _volume: &str,
        _archive: &VolumeArchive,
        _receive_root: &str,
    ) -> BR<()> {
        Ok(())
    }
}

#[test]
fn test_executor_default_print_methods_emit_logs() {
    let output = capture_logs(|| {
        let executor = DummyExecutor;
        executor.print_info("hello");
        executor.print_warn("careful");
        executor.print_error("boom");
    });

    assert!(output.contains("hello"));
    assert!(output.contains("WARN"));
    assert!(output.contains("careful"));
    assert!(output.contains("ERROR"));
    assert!(output.contains("boom"));
}

#[test]
fn test_summon_real_executor_and_inspect_invalid_source() {
    let executor = summon_real_executor().unwrap();
    let result = executor.inspect_source(&SourceSpec {
        path: "/definitely/missing".to_string(),
        filter: vec!["*".to_string()],
    });

    match result {
        Err(err) => match err {
            BbkarError::InvalidSourcePath(path) => assert_eq!(path, "/definitely/missing"),
            other => panic!("unexpected error: {other}"),
        },
        Ok(_) => panic!("expected invalid source path error"),
    }
}

#[test]
fn test_real_executor_local_backend_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let dest_spec = local_dest(tmp.path());
    let executor = RealExecutor::new().unwrap();

    let missing = executor.inspect_dest_volume(&dest_spec, "vol").unwrap();
    assert!(missing.meta.is_none());

    let measure = executor
        .measure_subvolume(Box::new(
            vec![
                Ok(BtrfsSendChunk::StdoutBytes(b"hello world".to_vec(), 0)),
                Ok(BtrfsSendChunk::ProcessExit(0, String::new())),
            ]
            .into_iter(),
        ))
        .unwrap();
    assert_eq!(measure.uncompressed, 11);
    assert!(measure.compressed > 0);

    let (chunks, stats) = executor
        .write_subvolume(
            &dest_spec,
            "vol",
            "20230101",
            1,
            Box::new(
                vec![
                    Ok(BtrfsSendChunk::StdoutBytes(b"hello world".to_vec(), 0)),
                    Ok(BtrfsSendChunk::ProcessExit(0, String::new())),
                ]
                .into_iter(),
            ),
        )
        .unwrap();
    assert_eq!(stats.raw_bytes, 11);
    assert!(stats.compressed_bytes > 0);
    assert_eq!(chunks.len(), 1);

    let archive = VolumeArchive {
        timestamp: snap("20230101"),
        parent_timestamp: None,
        chunks,
    };
    let meta = DestMeta::new(1000, 2000, vec![archive]);
    executor.write_metadata(&dest_spec, "vol", &meta).unwrap();

    let state = executor.inspect_dest_volume(&dest_spec, "vol").unwrap();
    let loaded = state.meta.unwrap();
    assert_eq!(loaded.first_sync_timestamp, 1000);
    assert_eq!(loaded.last_sync_timestamp, 2000);
    assert_eq!(loaded.archives().len(), 1);
    assert_eq!(loaded.archives()[0].timestamp.raw(), "20230101");
    assert_eq!(loaded.archives()[0].chunks.len(), 1);

    let chunk_path = tmp
        .path()
        .join("vol")
        .join("20230101")
        .join(loaded.archives()[0].chunks[0].filename());
    assert!(chunk_path.is_file());
    assert!(tmp.path().join("vol").join("bbkar-meta.yaml").is_file());
}

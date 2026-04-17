#[allow(clippy::module_inception)]
mod cli;
mod dryrun;
mod ls;
mod restore;
mod run;
mod status;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::model::config::{BbkarConfigFile, DestSpec, GlobalConfig, SourceSpec};
use crate::model::dest::{DestMeta, VolumeArchive};
use crate::model::error::BR;
use crate::model::policy::{ResolvedSyncPolicy, RetentionPolicy, SendPolicy};
use crate::service::executor::Executor;
use crate::service::executor::inspect_source::SourceState;
use crate::utils::format::format_bytes;
use crate::utils::wildcard::wildcard_match;
use tracing::{debug, info, trace};

pub use cli::{Cli, Commands};
pub use dryrun::dryrun;
pub use ls::ls;
pub use restore::{dryrestore, restore};
pub use run::run;
pub use status::status;

pub struct VolumeContext<'a> {
    pub sync_name: &'a str,
    pub global: &'a GlobalConfig,
    pub src_spec: &'a SourceSpec,
    pub dest_spec: &'a DestSpec,
    pub volume: &'a str,
    pub src_state: &'a SourceState,
    pub send_policy: SendPolicy,
    pub retention_policy: RetentionPolicy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ArchiveStats {
    all_count: usize,
    all_size: u64,
    full_count: usize,
    full_size: u64,
    incremental_count: usize,
    incremental_size: u64,
}

impl ArchiveStats {
    fn from_meta(meta: Option<&DestMeta>) -> Self {
        let mut stats = Self::default();
        if let Some(meta) = meta {
            for archive in meta.archives() {
                stats.add_archive(archive);
            }
        }
        stats
    }

    fn add_archive(&mut self, archive: &VolumeArchive) {
        self.add_size(archive.is_incremental(), archive.total_size());
    }

    fn add_size(&mut self, incremental: bool, size: u64) {
        self.all_count += 1;
        self.all_size += size;
        if incremental {
            self.incremental_count += 1;
            self.incremental_size += size;
        } else {
            self.full_count += 1;
            self.full_size += size;
        }
    }

    fn merge(&mut self, other: Self) {
        self.all_count += other.all_count;
        self.all_size += other.all_size;
        self.full_count += other.full_count;
        self.full_size += other.full_size;
        self.incremental_count += other.incremental_count;
        self.incremental_size += other.incremental_size;
    }
}

fn print_archive_stats(executor: &dyn Executor, indent: &str, label: &str, stats: ArchiveStats) {
    executor.print_info(&format!(
        "{indent}{label}: {} archives, {} (full: {} archives, {}; incremental: {} archives, {})",
        stats.all_count,
        format_bytes(stats.all_size),
        stats.full_count,
        format_bytes(stats.full_size),
        stats.incremental_count,
        format_bytes(stats.incremental_size)
    ));
}

fn for_each_sync(
    path: &Path,
    executor: &dyn Executor,
    sync_name_filter: Option<&str>,
    mut on_sync_start: impl FnMut(&str, &SourceSpec, &DestSpec, &SendPolicy) -> BR<()>,
    mut on_volume: impl FnMut(&VolumeContext) -> BR<()>,
    mut on_sync_end: impl FnMut(&str, &SourceSpec, &DestSpec, &SendPolicy) -> BR<()>,
) -> BR<()> {
    debug!(config_path = %path.display(), "loading config");
    let config = load_config(path)?;
    debug!(
        sources = config.source.len(),
        destinations = config.dest.len(),
        syncs = config.sync.len(),
        "config loaded"
    );

    if let Some(name) = sync_name_filter {
        if !config.sync.contains_key(name) {
            let names: Vec<&str> = config.sync.keys().map(|s| s.as_str()).collect();
            return Err(crate::model::error::BbkarError::Config(vec![format!(
                "sync '{}' not found in config (available: {})",
                name,
                names.join(", ")
            )]));
        }
        info!(sync = %name, "using sync (--name)");
    }

    let mut source_cache: HashMap<&str, HashMap<String, SourceState>> = HashMap::new();

    for (sync_name, sync_spec) in config.sync.iter() {
        if let Some(name) = sync_name_filter
            && sync_name != name
        {
            continue;
        }
        executor.print_info(&format!("[sync.{}]", sync_name));
        let src_spec = &config.source[&sync_spec.source];
        let dest_spec = &config.dest[&sync_spec.dest];
        let policy = ResolvedSyncPolicy::from_sync_spec(sync_spec);
        on_sync_start(sync_name, src_spec, dest_spec, &policy.send)?;
        debug!(
            sync = %sync_name,
            source = %sync_spec.source,
            dest = %sync_spec.dest,
            filters = ?sync_spec.filter,
            max_incremental_depth = ?policy.send.max_incremental_depth,
            min_full_send_interval_days = policy.send.min_full_send_interval.days,
            "processing sync"
        );

        if !source_cache.contains_key(sync_spec.source.as_str()) {
            debug!(source = %sync_spec.source, path = %src_spec.path, "inspecting source");
            let states = executor.inspect_source(src_spec)?;
            debug!(
                source = %sync_spec.source,
                volumes = states.len(),
                "source inspection complete"
            );
            source_cache.insert(&sync_spec.source, states);
        }
        let src_volume_states = &source_cache[sync_spec.source.as_str()];

        for (volume, src_state) in src_volume_states.iter() {
            if !sync_spec.filter.iter().any(|p| wildcard_match(p, volume)) {
                trace!(sync = %sync_name, volume = %volume, "volume filtered out");
                continue;
            }
            executor.print_info(&format!(
                "  volume: {}.* -> {}/{}.*",
                src_spec.build_path(volume),
                dest_spec.display_location(),
                volume
            ));
            debug!(
                sync = %sync_name,
                volume = %volume,
                snapshots = src_state.volume.snapshots().len(),
                oldest = %src_state.volume.oldest_snapshot().raw(),
                newest = %src_state.volume.newest_snapshot().raw(),
                dest = %dest_spec.display_location(),
                "dispatching volume"
            );

            on_volume(&VolumeContext {
                sync_name,
                global: &config.global,
                src_spec,
                dest_spec,
                volume,
                src_state,
                send_policy: policy.send.clone(),
                retention_policy: policy.retention.clone(),
            })?;
        }
        on_sync_end(sync_name, src_spec, dest_spec, &policy.send)?;
    }
    Ok(())
}

fn for_each_volume(
    path: &Path,
    executor: &dyn Executor,
    sync_name_filter: Option<&str>,
    mut on_volume: impl FnMut(&VolumeContext) -> BR<()>,
) -> BR<()> {
    for_each_sync(
        path,
        executor,
        sync_name_filter,
        |_, _, _, _| Ok(()),
        |ctx| on_volume(ctx),
        |_, _, _, _| Ok(()),
    )
}

fn load_config(path: &Path) -> BR<BbkarConfigFile> {
    let content = fs::read_to_string(path)?;
    BbkarConfigFile::from_toml(&content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BtrfsSendChunk;
    use crate::model::config::BackendSpec;
    use crate::model::dest::{ChunkFilename, DestState};
    use crate::model::error::BbkarError;
    use crate::model::source::{Series, Timestamp};
    use crate::service::executor::measure::MeasureResult;
    use crate::service::executor::write_dest::TransferStats;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn snap(name: &str) -> Timestamp {
        Timestamp::parse(name).unwrap()
    }

    fn chunk(name: &str, size: u32) -> ChunkFilename {
        ChunkFilename::new(
            name.to_string(),
            size,
            Some("zstd".to_string()),
            Some(size as u64),
            Some("deadbeef".to_string()),
        )
    }

    fn source_state(volume_name: &str, names: &[&str]) -> SourceState {
        SourceState {
            volume: Series::new(
                volume_name.to_string(),
                names.iter().map(|n| snap(n)).collect(),
            ),
        }
    }

    fn write_config(dir: &tempfile::TempDir, filter: &[&str]) -> std::path::PathBuf {
        let filter_toml = filter
            .iter()
            .map(|entry| format!("\"{}\"", entry))
            .collect::<Vec<_>>()
            .join(", ");
        let content = format!(
            r#"[global]

[source.src1]
path = "/fake/source"

[dest.dst1]
driver = "local"
path = "/fake/dest"

[sync.main]
source = "src1"
dest = "dst1"
filter = [{}]
"#,
            filter_toml
        );
        let path = dir.path().join("config.toml");
        std::fs::write(&path, content).unwrap();
        path
    }

    struct RecordingExecutor {
        sources: HashMap<String, SourceState>,
        printed: Rc<RefCell<Vec<String>>>,
    }

    impl RecordingExecutor {
        fn new(sources: HashMap<String, SourceState>) -> (Self, Rc<RefCell<Vec<String>>>) {
            let printed = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    sources,
                    printed: printed.clone(),
                },
                printed,
            )
        }
    }

    impl Executor for RecordingExecutor {
        fn inspect_source(&self, _src: &SourceSpec) -> BR<HashMap<String, SourceState>> {
            Ok(self.sources.clone())
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
        ) -> BR<(Vec<ChunkFilename>, TransferStats)> {
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
            _archive: &crate::model::dest::VolumeArchive,
            _receive_root: &str,
        ) -> BR<()> {
            Ok(())
        }

        fn print_info(&self, info: &str) {
            self.printed.borrow_mut().push(info.to_string());
        }
    }

    #[test]
    fn test_archive_stats_from_meta_and_merge() {
        let left = DestMeta::new(
            1,
            2,
            vec![
                VolumeArchive {
                    timestamp: snap("20230101"),
                    parent_timestamp: None,
                    chunks: vec![chunk("part1", 10)],
                },
                VolumeArchive {
                    timestamp: snap("20230102"),
                    parent_timestamp: Some("20230101".to_string()),
                    chunks: vec![chunk("part2", 4)],
                },
            ],
        );
        let right = DestMeta::new(
            1,
            2,
            vec![VolumeArchive {
                timestamp: snap("20230103"),
                parent_timestamp: None,
                chunks: vec![chunk("part3", 7)],
            }],
        );

        let mut stats = ArchiveStats::from_meta(Some(&left));
        stats.merge(ArchiveStats::from_meta(Some(&right)));

        assert_eq!(stats.all_count, 3);
        assert_eq!(stats.all_size, 21);
        assert_eq!(stats.full_count, 2);
        assert_eq!(stats.full_size, 17);
        assert_eq!(stats.incremental_count, 1);
        assert_eq!(stats.incremental_size, 4);
    }

    #[test]
    fn test_print_archive_stats_formats_expected_summary() {
        let (executor, printed) = RecordingExecutor::new(HashMap::new());
        print_archive_stats(
            &executor,
            "  ",
            "remote usage",
            ArchiveStats {
                all_count: 2,
                all_size: 1024,
                full_count: 1,
                full_size: 768,
                incremental_count: 1,
                incremental_size: 256,
            },
        );

        let lines = printed.borrow();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("remote usage: 2 archives,"));
        assert!(lines[0].contains("(full: 1 archives,"));
        assert!(lines[0].contains("incremental: 1 archives,"));
    }

    #[test]
    fn test_load_config_returns_io_error_for_missing_file() {
        let err = load_config(Path::new("/definitely/missing/bbkar.toml")).unwrap_err();
        assert!(matches!(err, BbkarError::Io(_)));
    }

    #[test]
    fn test_for_each_sync_invokes_start_volume_end_for_matching_volumes() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db*"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        sources.insert("home".to_string(), source_state("home", &["20230101"]));
        let (executor, printed) = RecordingExecutor::new(sources);

        let calls = RefCell::new(Vec::new());
        for_each_sync(
            &config_path,
            &executor,
            None,
            |sync_name, src, dest, send| {
                calls.borrow_mut().push(format!(
                    "start:{sync_name}:{}:{}:{}",
                    src.path,
                    dest.display_location(),
                    send.describe()
                ));
                Ok(())
            },
            |ctx| {
                calls.borrow_mut().push(format!(
                    "volume:{}:{}:{}",
                    ctx.sync_name,
                    ctx.volume,
                    ctx.send_policy.describe()
                ));
                Ok(())
            },
            |sync_name, _, _, _| {
                calls.borrow_mut().push(format!("end:{sync_name}"));
                Ok(())
            },
        )
        .unwrap();

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].starts_with("start:main:/fake/source:/fake/dest:"));
        assert_eq!(
            calls[1],
            "volume:main:db:full at least every 1w, no incremental depth limit"
        );
        assert_eq!(calls[2], "end:main");

        let printed = printed.borrow();
        assert!(printed.iter().any(|line| line == "[sync.main]"));
        assert!(
            printed
                .iter()
                .any(|line| line.contains("volume: /fake/source/db.* -> /fake/dest/db.*"))
        );
        assert!(!printed.iter().any(|line| line.contains("home.*")));
    }

    #[test]
    fn test_for_each_volume_wraps_for_each_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        let (executor, _printed) = RecordingExecutor::new(sources);
        let mut seen = Vec::new();

        for_each_volume(&config_path, &executor, None, |ctx| {
            seen.push(format!(
                "{}:{}:{}",
                ctx.sync_name,
                ctx.volume,
                ctx.dest_spec.display_location()
            ));
            Ok(())
        })
        .unwrap();

        assert_eq!(seen, vec!["main:db:/fake/dest"]);
    }

    #[test]
    fn test_for_each_sync_propagates_callback_error() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        let (executor, _printed) = RecordingExecutor::new(sources);

        let err = for_each_sync(
            &config_path,
            &executor,
            None,
            |_, _, _, _| Ok(()),
            |_| Err(BbkarError::Execution("stop".into())),
            |_, _, _, _| Ok(()),
        )
        .unwrap_err();

        assert!(format!("{err}").contains("stop"));
    }

    #[test]
    fn test_for_each_sync_uses_selected_name() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        let (executor, _printed) = RecordingExecutor::new(sources);
        let seen = RefCell::new(Vec::new());

        for_each_sync(
            &config_path,
            &executor,
            Some("main"),
            |sync_name, _, _, _| {
                seen.borrow_mut().push(format!("start:{sync_name}"));
                Ok(())
            },
            |ctx| {
                seen.borrow_mut().push(format!("volume:{}", ctx.volume));
                Ok(())
            },
            |sync_name, _, _, _| {
                seen.borrow_mut().push(format!("end:{sync_name}"));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            seen.into_inner(),
            vec!["start:main", "volume:db", "end:main"]
        );
    }

    #[test]
    fn test_volume_context_carries_default_policies() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        let (executor, _printed) = RecordingExecutor::new(sources);
        let mut descriptions = Vec::new();

        for_each_volume(&config_path, &executor, None, |ctx| {
            descriptions.push(ctx.send_policy.describe());
            descriptions.push(ctx.retention_policy.describe());
            Ok(())
        })
        .unwrap();

        assert_eq!(
            descriptions,
            vec![
                "full at least every 1w, no incremental depth limit".to_string(),
                "keep all archives".to_string()
            ]
        );
    }

    #[test]
    fn test_volume_context_dest_spec_is_local_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = write_config(&tmp, &["db"]);
        let mut sources = HashMap::new();
        sources.insert("db".to_string(), source_state("db", &["20230101"]));
        let (executor, _printed) = RecordingExecutor::new(sources);
        let mut backend_is_local = false;

        for_each_volume(&config_path, &executor, None, |ctx| {
            backend_is_local = matches!(ctx.dest_spec.backend_spec(), BackendSpec::Local { .. });
            Ok(())
        })
        .unwrap();

        assert!(backend_is_local);
    }
}

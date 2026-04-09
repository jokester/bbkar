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
use crate::model::policy::{ResolvedSyncPolicy, SendPolicy};
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
        if let Some(name) = sync_name_filter {
            if sync_name != name {
                continue;
            }
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

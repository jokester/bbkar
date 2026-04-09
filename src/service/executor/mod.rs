mod btrfs_send;
mod compress;
mod r#impl;
mod inspect_dest;
pub mod inspect_source;
pub mod measure;
mod opendal;
pub mod read_dest;
mod sudo;
pub mod write_dest;

use std::collections::HashMap;

use crate::model::BtrfsSendChunk;
use crate::model::config::{DestSpec, SourceSpec};
use crate::model::dest::{ChunkFilename, DestMeta, DestState, VolumeArchive};
use crate::model::error::BR;
use write_dest::TransferStats;

use inspect_source::SourceState;
use tracing::{error, info, warn};

pub use r#impl::RealExecutor;

/**
 * Executor: provides IO primitives
 */
pub trait Executor {
    fn inspect_source(&self, src: &SourceSpec) -> BR<HashMap<String, SourceState>>;

    fn inspect_dest_volume(&self, spec: &DestSpec, volume: &str) -> BR<DestState>;

    /**
     * stream bytes from `btrfs send`
     */
    fn read_subvolume_full(
        &self,
        src: SourceSpec,
        subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>;

    /**
     * stream bytes from `btrfs send --parent`
     */
    fn read_subvolume_incremental(
        &self,
        src: SourceSpec,
        subvolume: &str,
        parent_subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>;

    fn write_metadata(
        &self,
        dest_spec: &DestSpec,
        volume_basename: &str,
        new_meta: &DestMeta,
    ) -> BR<()>;

    fn write_subvolume(
        &self,
        dest_spec: &DestSpec,
        volume_basename: &str,
        snapshot: &str,
        max_chunk_size_mib: u64,
        chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<(Vec<ChunkFilename>, TransferStats)>;

    // instead of write to store, read the bytes and return uncompressed/compressed sizes
    fn measure_subvolume(
        &self,
        chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<measure::MeasureResult>;

    /// Read archive chunks from dest, decompress, and pipe to `btrfs receive`
    fn restore_archive(
        &self,
        dest_spec: &DestSpec,
        volume: &str,
        archive: &VolumeArchive,
        receive_root: &str,
    ) -> BR<()>;

    fn print_info(&self, info: &str) {
        info!("{}", info);
    }

    fn print_warn(&self, warn: &str) {
        warn!("{}", warn);
    }

    fn print_error(&self, error: &str) {
        error!("{}", error);
    }
}

pub fn summon_real_executor() -> BR<RealExecutor> {
    RealExecutor::new()
}

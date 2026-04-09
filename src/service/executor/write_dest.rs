use std::cell::Cell;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use opendal::blocking::StdWriter;
use sha2::{Digest, Sha256};
use tracing::{debug, trace};

use crate::model::BtrfsSendChunk;
use crate::model::config::{BackendSpec, DestSpec};
use crate::model::dest::{ChunkFilename, DestMeta};
use crate::model::error::{BR, BbkarError};

use crate::utils::format::format_speed;

use super::compress::CompressSession;
use super::opendal::{path_in_snapshot, path_in_volume, summon_blocking_operator};

/// Per-snapshot transfer timing breakdown.
pub struct TransferStats {
    pub elapsed: Duration,
    pub raw_bytes: u64,
    pub compressed_bytes: u64,
    pub read_time: Duration,
    pub write_time: Duration,
}

impl TransferStats {
    pub fn compress_time(&self) -> Duration {
        self.elapsed
            .saturating_sub(self.read_time + self.write_time)
    }
}

const META_FILENAME: &str = "bbkar-meta.yaml";

enum ChunkOutput {
    Local(File),
    Remote(Box<StdWriter>),
}

impl ChunkOutput {
    fn close(&mut self) -> std::io::Result<()> {
        match self {
            Self::Local(file) => file.flush(),
            Self::Remote(writer) => writer.close(),
        }
    }
}

impl Write for ChunkOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Local(file) => file.write(buf),
            Self::Remote(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Local(file) => file.flush(),
            Self::Remote(writer) => writer.flush(),
        }
    }
}

/// A `Write` sink that splits compressed output into numbered chunk files.
///
/// Files are written as `DEST/VOLUME/SNAPSHOT/part000001.btrfs.zstd`, etc.
pub struct ChunkedWriter {
    dest_spec: DestSpec,
    volume: String,
    snapshot: String,
    max_chunk_bytes: u64,
    current_file: Option<ChunkOutput>,
    current_filename: String,
    current_size: u64,
    part_number: u32,
    written_chunks: Vec<ChunkFilename>,
    current_hasher: Sha256,
    raw_counter: Rc<Cell<u64>>,
    raw_at_last_roll: u64,
    write_nanos: Rc<Cell<u64>>,
    read_nanos: Rc<Cell<u64>>,
    chunk_start: Instant,
    write_nanos_at_chunk_start: u64,
    read_nanos_at_chunk_start: u64,
}

impl ChunkedWriter {
    pub fn new(
        dest_spec: DestSpec,
        volume: String,
        snapshot: String,
        max_chunk_bytes: u64,
        raw_counter: Rc<Cell<u64>>,
        write_nanos: Rc<Cell<u64>>,
        read_nanos: Rc<Cell<u64>>,
    ) -> Self {
        Self {
            dest_spec,
            volume,
            snapshot,
            max_chunk_bytes,
            current_file: None,
            current_filename: String::new(),
            current_size: 0,
            part_number: 0,
            written_chunks: Vec::new(),
            current_hasher: Sha256::new(),
            raw_counter,
            raw_at_last_roll: 0,
            write_nanos,
            read_nanos,
            chunk_start: Instant::now(),
            write_nanos_at_chunk_start: 0,
            read_nanos_at_chunk_start: 0,
        }
    }

    fn open_next_chunk(&self, filename: &str) -> BR<ChunkOutput> {
        debug!(
            volume = %self.volume,
            snapshot = %self.snapshot,
            filename = %filename,
            "opening output chunk"
        );
        match self.dest_spec.backend_spec() {
            BackendSpec::Local { path } => {
                let snapshot_dir = PathBuf::from(path).join(&self.volume).join(&self.snapshot);
                fs::create_dir_all(&snapshot_dir)?;
                Ok(ChunkOutput::Local(File::create(
                    snapshot_dir.join(filename),
                )?))
            }
            BackendSpec::S3 { .. } | BackendSpec::Gcs { .. } => {
                let op = summon_blocking_operator(&self.dest_spec)?;
                let object_path = path_in_snapshot(&self.volume, &self.snapshot, filename);
                let writer = op.writer(&object_path)?.into_std_write();
                Ok(ChunkOutput::Remote(Box::new(writer)))
            }
        }
    }

    fn roll_next(&mut self) -> BR<()> {
        self.finalize_current()?;
        self.part_number += 1;
        let filename = format!("part{:06}.btrfs.zstd", self.part_number);
        debug!(
            volume = %self.volume,
            snapshot = %self.snapshot,
            part = self.part_number,
            max_chunk_bytes = self.max_chunk_bytes,
            "rolling to next chunk"
        );
        let file = self.open_next_chunk(&filename)?;
        self.current_file = Some(file);
        self.current_filename = filename;
        self.current_size = 0;
        self.current_hasher = Sha256::new();
        self.chunk_start = Instant::now();
        self.write_nanos_at_chunk_start = self.write_nanos.get();
        self.read_nanos_at_chunk_start = self.read_nanos.get();
        Ok(())
    }

    fn finalize_current(&mut self) -> BR<()> {
        if let Some(ref mut file) = self.current_file {
            file.close().map_err(BbkarError::Io)?;
            let hasher = std::mem::replace(&mut self.current_hasher, Sha256::new());
            let sha256sum = hasher
                .finalize()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            let raw_size = self.raw_counter.get() - self.raw_at_last_roll;
            self.raw_at_last_roll = self.raw_counter.get();

            let chunk_elapsed = self.chunk_start.elapsed();
            let chunk_read =
                Duration::from_nanos(self.read_nanos.get() - self.read_nanos_at_chunk_start);
            let chunk_write =
                Duration::from_nanos(self.write_nanos.get() - self.write_nanos_at_chunk_start);
            let chunk_compress = chunk_elapsed.saturating_sub(chunk_read + chunk_write);
            debug!(
                volume = %self.volume,
                snapshot = %self.snapshot,
                filename = %self.current_filename,
                compressed_size = self.current_size,
                raw_size,
                elapsed_ms = chunk_elapsed.as_millis() as u64,
                "chunk done: send {} | compress {} | write {}",
                format_speed(raw_size, chunk_read),
                format_speed(raw_size, chunk_compress),
                format_speed(self.current_size, chunk_write),
            );
            self.written_chunks.push(ChunkFilename::new(
                self.current_filename.clone(),
                self.current_size as u32,
                Some("zstd".to_string()),
                Some(raw_size),
                Some(sha256sum),
            ));
        }
        self.current_file = None;
        Ok(())
    }

    pub fn finish(mut self) -> BR<Vec<ChunkFilename>> {
        let t = Instant::now();
        self.finalize_current()?;
        self.write_nanos
            .set(self.write_nanos.get() + t.elapsed().as_nanos() as u64);
        Ok(self.written_chunks)
    }
}

impl Write for ChunkedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let t = Instant::now();
        if self.current_file.is_none() || self.current_size >= self.max_chunk_bytes {
            self.roll_next()
                .map_err(|err| std::io::Error::other(err.to_string()))?;
        }
        let file = self.current_file.as_mut().unwrap();
        let written = file.write(buf)?;
        self.write_nanos
            .set(self.write_nanos.get() + t.elapsed().as_nanos() as u64);
        self.current_hasher.update(&buf[..written]);
        self.current_size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(ref mut file) = self.current_file {
            file.flush()?;
        }
        Ok(())
    }
}

/// Write a subvolume's btrfs-send stream (compressed) to chunk files in dest.
pub fn write_subvolume_to_dest(
    dest_spec: &DestSpec,
    volume: &str,
    snapshot: &str,
    max_chunk_size_mib: u64,
    chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
) -> BR<(Vec<ChunkFilename>, TransferStats)> {
    let max_chunk_bytes = max_chunk_size_mib * 1024 * 1024;
    debug!(
        volume = %volume,
        snapshot = %snapshot,
        max_chunk_bytes,
        "writing compressed stream to destination"
    );
    let raw_counter = Rc::new(Cell::new(0u64));
    let write_nanos = Rc::new(Cell::new(0u64));
    let read_nanos = Rc::new(Cell::new(0u64));
    let writer = ChunkedWriter::new(
        dest_spec.clone(),
        volume.to_string(),
        snapshot.to_string(),
        max_chunk_bytes,
        raw_counter.clone(),
        write_nanos.clone(),
        read_nanos.clone(),
    );
    let session = CompressSession::new(writer)?;
    let raw_counter_for_chunks = raw_counter.clone();
    let read_nanos_for_chunks = read_nanos.clone();
    let timed_chunks = chunks.map(move |chunk| {
        if let Ok(BtrfsSendChunk::StdoutBytes(ref data, _)) = chunk {
            raw_counter_for_chunks.set(raw_counter_for_chunks.get() + data.len() as u64);
        }
        chunk
    });
    // Wrap iterator to measure time spent reading from btrfs send
    let timed_chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> = Box::new(TimedIterator::new(
        Box::new(timed_chunks),
        read_nanos_for_chunks,
    ));
    let start = Instant::now();
    let writer = session.process_all(timed_chunks)?;
    let chunk_files = writer.finish()?;
    let elapsed = start.elapsed();

    let compressed_bytes: u64 = chunk_files.iter().map(|c| c.size() as u64).sum();
    let stats = TransferStats {
        elapsed,
        raw_bytes: raw_counter.get(),
        compressed_bytes,
        read_time: Duration::from_nanos(read_nanos.get()),
        write_time: Duration::from_nanos(write_nanos.get()),
    };
    debug!(
        volume = %volume,
        snapshot = %snapshot,
        chunks = chunk_files.len(),
        raw_size = raw_counter.get(),
        elapsed_ms = elapsed.as_millis() as u64,
        "completed subvolume write"
    );
    Ok((chunk_files, stats))
}

/// Iterator wrapper that accumulates time spent in `next()`.
struct TimedIterator {
    inner: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    nanos: Rc<Cell<u64>>,
}

impl TimedIterator {
    fn new(inner: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>, nanos: Rc<Cell<u64>>) -> Self {
        Self { inner, nanos }
    }
}

impl Iterator for TimedIterator {
    type Item = BR<BtrfsSendChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        let t = Instant::now();
        let result = self.inner.next();
        self.nanos
            .set(self.nanos.get() + t.elapsed().as_nanos() as u64);
        result
    }
}

/// Write metadata to `DEST/VOLUME/bbkar-meta.yaml`.
pub fn write_metadata_to_dest(dest_spec: &DestSpec, volume: &str, meta: &DestMeta) -> BR<()> {
    let content = serde_yaml::to_string(meta)?;
    debug!(
        volume = %volume,
        dest = %dest_spec.display_location(),
        bytes = content.len(),
        archives = meta.archives().len(),
        "persisting metadata"
    );

    match dest_spec.backend_spec() {
        BackendSpec::Local { path } => {
            let volume_dir = PathBuf::from(path).join(volume);
            fs::create_dir_all(&volume_dir)?;

            let meta_path = volume_dir.join(META_FILENAME);
            let tmp_path = volume_dir.join(".bbkar-meta.yaml.tmp");

            fs::write(&tmp_path, content.as_bytes())?;
            fs::rename(&tmp_path, &meta_path)?;
        }
        BackendSpec::S3 { .. } | BackendSpec::Gcs { .. } => {
            let op = summon_blocking_operator(dest_spec)?;
            let meta_path = path_in_volume(volume, META_FILENAME);
            trace!(volume = %volume, meta_path = %meta_path, "writing metadata via OpenDAL");
            op.write(&meta_path, content.into_bytes())?;
        }
    }

    Ok(())
}

use std::io::{self, Cursor, Read};

use tracing::debug;

use crate::model::config::DestSpec;
use crate::model::dest::VolumeArchive;
use crate::model::error::BR;

use super::opendal::{path_in_snapshot, summon_blocking_operator};

/// A `Read` adapter that streams through concatenated archive chunk files.
///
/// Reads chunk files in order from the destination storage, presenting them
/// as a single continuous byte stream. This is the inverse of `ChunkedWriter`.
pub struct ChunkStreamReader {
    op: opendal::blocking::Operator,
    volume: String,
    snapshot_raw: String,
    chunk_filenames: Vec<String>,
    current_idx: usize,
    current_cursor: Option<Cursor<Vec<u8>>>,
}

impl ChunkStreamReader {
    pub fn new(dest_spec: &DestSpec, volume: &str, archive: &VolumeArchive) -> BR<Self> {
        let op = summon_blocking_operator(dest_spec)?;
        let chunk_filenames: Vec<String> = archive
            .chunks
            .iter()
            .map(|c| c.filename().to_string())
            .collect();
        debug!(
            volume = %volume,
            snapshot = %archive.timestamp.raw(),
            chunks = chunk_filenames.len(),
            "created chunk stream reader"
        );
        Ok(Self {
            op,
            volume: volume.to_string(),
            snapshot_raw: archive.timestamp.raw().to_string(),
            chunk_filenames,
            current_idx: 0,
            current_cursor: None,
        })
    }

    fn load_next_chunk(&mut self) -> io::Result<bool> {
        if self.current_idx >= self.chunk_filenames.len() {
            return Ok(false);
        }
        let chunk_name = &self.chunk_filenames[self.current_idx];
        let path = path_in_snapshot(&self.volume, &self.snapshot_raw, chunk_name);
        debug!(
            volume = %self.volume,
            snapshot = %self.snapshot_raw,
            chunk = %chunk_name,
            path = %path,
            "loading chunk"
        );
        let data = self
            .op
            .read(&path)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.current_cursor = Some(Cursor::new(data.to_vec()));
        self.current_idx += 1;
        Ok(true)
    }
}

impl Read for ChunkStreamReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if let Some(ref mut cursor) = self.current_cursor {
                let n = cursor.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
            }
            if !self.load_next_chunk()? {
                return Ok(0);
            }
        }
    }
}

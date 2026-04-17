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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::BackendSpec;
    use crate::model::dest::ChunkFilename;
    use crate::model::source::Timestamp;

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

    fn archive(snapshot: &str, chunk_names: &[&str]) -> VolumeArchive {
        VolumeArchive {
            timestamp: snap(snapshot),
            parent_timestamp: None,
            chunks: chunk_names
                .iter()
                .map(|name| ChunkFilename::new(name.to_string(), 0, None, None, None))
                .collect(),
        }
    }

    fn write_chunk(root: &std::path::Path, volume: &str, snapshot: &str, name: &str, data: &[u8]) {
        let dir = root.join(volume).join(snapshot);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), data).unwrap();
    }

    #[test]
    fn test_read_empty_archive_returns_eof() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = local_dest(tmp.path());
        let archive = archive("20230101", &[]);
        let mut reader = ChunkStreamReader::new(&spec, "vol", &archive).unwrap();
        let mut buf = [0u8; 8];

        let n = reader.read(&mut buf).unwrap();

        assert_eq!(n, 0);
    }

    #[test]
    fn test_read_single_chunk_stream() {
        let tmp = tempfile::tempdir().unwrap();
        write_chunk(
            tmp.path(),
            "vol",
            "20230101",
            "part000001.btrfs.zstd",
            b"hello",
        );
        let spec = local_dest(tmp.path());
        let archive = archive("20230101", &["part000001.btrfs.zstd"]);
        let mut reader = ChunkStreamReader::new(&spec, "vol", &archive).unwrap();
        let mut out = Vec::new();

        reader.read_to_end(&mut out).unwrap();

        assert_eq!(out, b"hello");
    }

    #[test]
    fn test_read_multiple_chunks_as_single_stream() {
        let tmp = tempfile::tempdir().unwrap();
        write_chunk(
            tmp.path(),
            "vol",
            "20230101",
            "part000001.btrfs.zstd",
            b"hello ",
        );
        write_chunk(
            tmp.path(),
            "vol",
            "20230101",
            "part000002.btrfs.zstd",
            b"world",
        );
        let spec = local_dest(tmp.path());
        let archive = archive(
            "20230101",
            &["part000001.btrfs.zstd", "part000002.btrfs.zstd"],
        );
        let mut reader = ChunkStreamReader::new(&spec, "vol", &archive).unwrap();
        let mut out = Vec::new();

        let mut buf = [0u8; 3];
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }

        assert_eq!(out, b"hello world");
    }

    #[test]
    fn test_read_missing_chunk_returns_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = local_dest(tmp.path());
        let archive = archive("20230101", &["part000001.btrfs.zstd"]);
        let mut reader = ChunkStreamReader::new(&spec, "vol", &archive).unwrap();
        let mut buf = [0u8; 8];

        let err = reader.read(&mut buf).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::Other);
    }
}

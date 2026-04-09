use std::io::Write;

use crate::model::BtrfsSendChunk;
use crate::model::error::BR;

/// A streaming zstd compression session.
///
/// Wraps a zstd encoder over an arbitrary `Write` sink. Feed `BtrfsSendChunk`s
/// through it, and call `finish()` to flush and retrieve the underlying writer.
///
/// Used by `measure_subvolume` (with a counting sink) and will be used by
/// `write_subvolume` (with a file/chunk sink).
pub struct CompressSession<W: Write> {
    encoder: zstd::stream::write::Encoder<'static, W>,
}

impl<W: Write> CompressSession<W> {
    pub fn new(writer: W) -> BR<Self> {
        let encoder = zstd::stream::write::Encoder::new(writer, 0)?;
        Ok(Self { encoder })
    }

    /// Compress a single chunk. Only `StdoutBytes` data is written;
    /// `ProcessExit` is ignored.
    pub fn write_chunk(&mut self, chunk: &BtrfsSendChunk) -> BR<()> {
        if let BtrfsSendChunk::StdoutBytes(data, _offset) = chunk {
            self.encoder.write_all(data)?;
        }
        Ok(())
    }

    /// Flush and finalize the zstd frame, returning the underlying writer.
    pub fn finish(self) -> BR<W> {
        Ok(self.encoder.finish()?)
    }

    /// Consume an entire chunk iterator, then finish.
    pub fn process_all(mut self, chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>) -> BR<W> {
        for chunk in chunks {
            self.write_chunk(&chunk?)?;
        }
        self.finish()
    }
}

/// A `Write` sink that counts bytes written without storing them.
pub struct CountingWriter(pub u64);

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_session_empty() {
        let session = CompressSession::new(CountingWriter(0)).unwrap();
        let chunks: Vec<BR<BtrfsSendChunk>> =
            vec![Ok(BtrfsSendChunk::ProcessExit(0, String::new()))];
        let counter = session.process_all(Box::new(chunks.into_iter())).unwrap();
        // zstd frame header is non-zero even for empty input
        assert!(counter.0 > 0);
    }

    #[test]
    fn test_compress_session_compresses() {
        let session = CompressSession::new(CountingWriter(0)).unwrap();
        let data = vec![0u8; 4096];
        let chunks: Vec<BR<BtrfsSendChunk>> = vec![
            Ok(BtrfsSendChunk::StdoutBytes(data, 0)),
            Ok(BtrfsSendChunk::ProcessExit(0, String::new())),
        ];
        let counter = session.process_all(Box::new(chunks.into_iter())).unwrap();
        assert!(counter.0 > 0);
        assert!(counter.0 < 4096);
    }

    #[test]
    fn test_compress_session_incremental_write() {
        let mut session = CompressSession::new(CountingWriter(0)).unwrap();
        session
            .write_chunk(&BtrfsSendChunk::StdoutBytes(vec![1, 2, 3], 0))
            .unwrap();
        session
            .write_chunk(&BtrfsSendChunk::StdoutBytes(vec![4, 5, 6], 3))
            .unwrap();
        // ProcessExit is a no-op
        session
            .write_chunk(&BtrfsSendChunk::ProcessExit(0, String::new()))
            .unwrap();
        let counter = session.finish().unwrap();
        assert!(counter.0 > 0);
    }
}

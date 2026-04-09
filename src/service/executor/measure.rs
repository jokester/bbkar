use crate::model::BtrfsSendChunk;
use crate::model::error::BR;
use tracing::debug;

use super::compress::{CompressSession, CountingWriter};

#[derive(Debug, Clone, Copy)]
pub struct MeasureResult {
    pub uncompressed: u64,
    pub compressed: u64,
}

pub fn measure_subvolume(
    chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
) -> BR<MeasureResult> {
    let mut uncompressed: u64 = 0;
    let mut session = CompressSession::new(CountingWriter(0))?;
    for chunk in chunks {
        let chunk = chunk?;
        if let BtrfsSendChunk::StdoutBytes(ref data, _) = chunk {
            uncompressed += data.len() as u64;
        }
        session.write_chunk(&chunk)?;
    }
    let counter = session.finish()?;
    let result = MeasureResult {
        uncompressed,
        compressed: counter.0,
    };
    debug!(
        uncompressed = result.uncompressed,
        compressed = result.compressed,
        "measured subvolume stream"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_empty_stream() {
        let chunks: Vec<BR<BtrfsSendChunk>> =
            vec![Ok(BtrfsSendChunk::ProcessExit(0, String::new()))];
        let result = measure_subvolume(Box::new(chunks.into_iter())).unwrap();
        assert_eq!(result.uncompressed, 0);
        // zstd frame header is non-zero even for empty input
        assert!(result.compressed > 0);
    }

    #[test]
    fn test_measure_some_bytes() {
        let data = vec![0u8; 4096];
        let chunks: Vec<BR<BtrfsSendChunk>> = vec![
            Ok(BtrfsSendChunk::StdoutBytes(data, 0)),
            Ok(BtrfsSendChunk::ProcessExit(0, String::new())),
        ];
        let result = measure_subvolume(Box::new(chunks.into_iter())).unwrap();
        assert_eq!(result.uncompressed, 4096);
        // compressed size should be much smaller than 4096 zeros
        assert!(result.compressed > 0);
        assert!(result.compressed < 4096);
    }
}

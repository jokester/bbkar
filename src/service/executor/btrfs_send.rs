use std::io::Read;

use crate::model::BtrfsSendChunk;
use crate::model::error::{BR, BbkarError};
use tracing::{debug, trace};

const READ_BUF_SIZE: usize = 1024 * 1024; // 1 MiB

pub struct BtrfsSendIterator {
    child: std::process::Child,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    offset: u64,
    done: bool,
}

impl BtrfsSendIterator {
    pub fn new(mut child: std::process::Child) -> Self {
        let stdout = child.stdout.take().expect("stdout was piped");
        debug!(pid = child.id(), "btrfs send process started");
        Self {
            child,
            stdout: std::io::BufReader::with_capacity(READ_BUF_SIZE, stdout),
            offset: 0,
            done: false,
        }
    }
}

impl Iterator for BtrfsSendIterator {
    type Item = BR<BtrfsSendChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let mut buf = vec![0u8; READ_BUF_SIZE];
        match self.stdout.read(&mut buf) {
            Ok(0) => {
                self.done = true;
                let stderr_str = self
                    .child
                    .stderr
                    .take()
                    .and_then(|mut stderr| {
                        let mut buf = String::new();
                        stderr.read_to_string(&mut buf).ok().map(|_| buf)
                    })
                    .unwrap_or_default();
                match self.child.wait() {
                    Ok(status) => {
                        let code = status.code().unwrap_or(1) as u32;
                        debug!(exit_code = code, stderr = %stderr_str, "btrfs send process finished");
                        Some(Ok(BtrfsSendChunk::ProcessExit(code, stderr_str)))
                    }
                    Err(e) => Some(Err(BbkarError::Io(e))),
                }
            }
            Ok(n) => {
                buf.truncate(n);
                let chunk = BtrfsSendChunk::StdoutBytes(buf, self.offset);
                trace!(offset = self.offset, bytes = n, "received btrfs send chunk");
                self.offset += n as u64;
                Some(Ok(chunk))
            }
            Err(e) => {
                self.done = true;
                Some(Err(BbkarError::Io(e)))
            }
        }
    }
}

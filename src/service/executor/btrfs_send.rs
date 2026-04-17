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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::process::{Command, Stdio};

    fn shell_child(script: &str) -> std::process::Child {
        Command::new("sh")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    #[test]
    fn test_iterator_yields_stdout_bytes_then_process_exit() {
        let child = shell_child("printf 'hello world'; printf 'warn' >&2");
        let mut iter = BtrfsSendIterator::new(child);

        let first = iter.next().unwrap().unwrap();
        let second = iter.next().unwrap().unwrap();

        match first {
            BtrfsSendChunk::StdoutBytes(bytes, offset) => {
                assert_eq!(bytes, b"hello world");
                assert_eq!(offset, 0);
            }
            other => panic!("unexpected first chunk: {}", chunk_kind(&other)),
        }

        match second {
            BtrfsSendChunk::ProcessExit(code, stderr) => {
                assert_eq!(code, 0);
                assert_eq!(stderr, "warn");
            }
            other => panic!("unexpected second chunk: {}", chunk_kind(&other)),
        }

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_iterator_tracks_offsets_across_multiple_reads() {
        let script = format!(
            "python3 - <<'PY'\nimport sys\nsys.stdout.write('a' * {})\nPY",
            READ_BUF_SIZE + 17
        );
        let child = shell_child(&script);
        let mut iter = BtrfsSendIterator::new(child);

        let mut reconstructed = Vec::new();
        let mut expected_offset = 0u64;
        let mut saw_multiple_stdout_chunks = false;

        loop {
            match iter.next().unwrap().unwrap() {
                BtrfsSendChunk::StdoutBytes(bytes, offset) => {
                    assert_eq!(offset, expected_offset);
                    expected_offset += bytes.len() as u64;
                    reconstructed.extend_from_slice(&bytes);
                    if expected_offset > bytes.len() as u64 {
                        saw_multiple_stdout_chunks = true;
                    }
                }
                BtrfsSendChunk::ProcessExit(code, stderr) => {
                    assert_eq!(code, 0);
                    assert!(stderr.is_empty());
                    break;
                }
            }
        }

        assert!(saw_multiple_stdout_chunks);
        assert_eq!(reconstructed.len(), READ_BUF_SIZE + 17);
        assert!(reconstructed.iter().all(|b| *b == b'a'));
    }

    #[test]
    fn test_iterator_reports_non_zero_exit_and_stderr() {
        let child = shell_child("printf 'partial'; printf 'boom' >&2; exit 7");
        let mut iter = BtrfsSendIterator::new(child);

        let first = iter.next().unwrap().unwrap();
        let second = iter.next().unwrap().unwrap();

        match first {
            BtrfsSendChunk::StdoutBytes(bytes, offset) => {
                assert_eq!(bytes, b"partial");
                assert_eq!(offset, 0);
            }
            other => panic!("unexpected first chunk: {}", chunk_kind(&other)),
        }

        match second {
            BtrfsSendChunk::ProcessExit(code, stderr) => {
                assert_eq!(code, 7);
                assert_eq!(stderr, "boom");
            }
            other => panic!("unexpected second chunk: {}", chunk_kind(&other)),
        }
    }

    #[test]
    fn test_iterator_reports_clean_exit_without_stdout() {
        let child = shell_child("printf 'warn-only' >&2");
        let mut iter = BtrfsSendIterator::new(child);

        match iter.next().unwrap().unwrap() {
            BtrfsSendChunk::ProcessExit(code, stderr) => {
                assert_eq!(code, 0);
                assert_eq!(stderr, "warn-only");
            }
            other => panic!("unexpected first chunk: {}", chunk_kind(&other)),
        }

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_iterator_maps_signal_exit_to_code_one() {
        let child = shell_child("kill -9 $$");
        let mut iter = BtrfsSendIterator::new(child);

        match iter.next().unwrap().unwrap() {
            BtrfsSendChunk::ProcessExit(code, stderr) => {
                assert_eq!(code, 1);
                assert!(stderr.is_empty());
            }
            other => panic!("unexpected first chunk: {}", chunk_kind(&other)),
        }
    }

    #[test]
    fn test_iterator_reports_stdout_read_error() {
        let child = shell_child("sleep 1");
        let mut iter = BtrfsSendIterator::new(child);

        let fd = iter.stdout.get_ref().as_raw_fd();
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            assert!(flags >= 0);
            assert_eq!(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK), 0);
        }

        match iter.next().unwrap() {
            Err(BbkarError::Io(_)) => {}
            Ok(BtrfsSendChunk::StdoutBytes(_, _)) => panic!("unexpected stdout chunk"),
            Ok(BtrfsSendChunk::ProcessExit(_, _)) => panic!("unexpected process exit"),
            Err(other) => panic!("unexpected error: {other}"),
        }

        let _ = iter.child.kill();
        let _ = iter.child.wait();
        assert!(iter.next().is_none());
    }

    fn chunk_kind(chunk: &BtrfsSendChunk) -> &'static str {
        match chunk {
            BtrfsSendChunk::StdoutBytes(_, _) => "stdout",
            BtrfsSendChunk::ProcessExit(_, _) => "exit",
        }
    }
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};

use super::write_dest::TransferStats;
use crate::model::BtrfsSendChunk;
use crate::model::config::{DestSpec, SourceSpec};
use crate::model::dest::{ChunkFilename, DestMeta, DestState, VolumeArchive};
use crate::model::error::{BR, BbkarError};
use tracing::debug;

use super::btrfs_send::BtrfsSendIterator;
use super::read_dest::ChunkStreamReader;
use super::sudo::SudoSession;
use super::{Executor, inspect_dest, inspect_source, measure, write_dest};
use inspect_source::SourceState;

pub struct RealExecutor {
    sudo_session: RefCell<Option<SudoSession>>,
}

impl RealExecutor {
    pub fn new() -> BR<Self> {
        Ok(Self {
            sudo_session: RefCell::new(None),
        })
    }

    fn ensure_sudo(&self) -> BR<()> {
        self.ensure_sudo_with(SudoSession::new, |session| session.ensure_active())
    }

    fn ensure_sudo_with(
        &self,
        create_session: impl FnOnce() -> BR<SudoSession>,
        refresh_session: impl FnOnce(&mut SudoSession) -> BR<()>,
    ) -> BR<()> {
        let mut session = self.sudo_session.borrow_mut();
        if session.is_none() {
            debug!("initializing sudo session");
            *session = Some(create_session()?);
        } else {
            debug!("refreshing sudo session");
            refresh_session(session.as_mut().unwrap())?;
        }
        Ok(())
    }

    fn btrfs_receive_command(&self, root: &str) -> BR<std::process::Child> {
        self.ensure_sudo()?;
        let needs_sudo = self.sudo_session.borrow().as_ref().unwrap().needs_sudo();
        debug!(root = %root, needs_sudo, "spawning btrfs receive");

        let child = build_btrfs_receive_command(root, needs_sudo).spawn();
        child.map_err(BbkarError::Io)
    }

    fn btrfs_send_command(&self, args: &[&str]) -> BR<std::process::Child> {
        self.ensure_sudo()?;
        let needs_sudo = self.sudo_session.borrow().as_ref().unwrap().needs_sudo();
        debug!(args = ?args, needs_sudo, "spawning btrfs send");

        let child = build_btrfs_send_command(args, needs_sudo).spawn();
        child.map_err(BbkarError::Io)
    }
}

fn build_btrfs_receive_command(root: &str, needs_sudo: bool) -> Command {
    let mut command = if needs_sudo {
        let mut command = Command::new("sudo");
        command.args(["btrfs", "receive", root]);
        command
    } else {
        let mut command = Command::new("btrfs");
        command.args(["receive", root]);
        command
    };
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());
    command
}

fn build_btrfs_send_command(args: &[&str], needs_sudo: bool) -> Command {
    let mut command = if needs_sudo {
        let mut command = Command::new("sudo");
        command.args(["btrfs", "send"]);
        command.args(args);
        command
    } else {
        let mut command = Command::new("btrfs");
        command.arg("send");
        command.args(args);
        command
    };
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command
}

impl Executor for RealExecutor {
    fn inspect_source(&self, src: &SourceSpec) -> BR<HashMap<String, SourceState>> {
        inspect_source::inspect_source(src)
    }

    fn inspect_dest_volume(&self, spec: &DestSpec, volume: &str) -> BR<DestState> {
        inspect_dest::inspect_dest_volume(spec, volume)
    }

    fn read_subvolume_full(
        &self,
        src: SourceSpec,
        subvolume: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> {
        let path = format!("{}/{}", src.path, subvolume);
        debug!(path = %path, "starting full send stream");
        match self.btrfs_send_command(&[&path]) {
            Ok(child) => Box::new(BtrfsSendIterator::new(child)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    fn read_subvolume_incremental(
        &self,
        src: SourceSpec,
        subvolume: &str,
        parent: &str,
    ) -> Box<dyn Iterator<Item = BR<BtrfsSendChunk>>> {
        let path = format!("{}/{}", src.path, subvolume);
        let parent_path = format!("{}/{}", src.path, parent);
        debug!(path = %path, parent = %parent_path, "starting incremental send stream");
        match self.btrfs_send_command(&["-p", &parent_path, &path]) {
            Ok(child) => Box::new(BtrfsSendIterator::new(child)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    fn write_metadata(
        &self,
        dest_spec: &DestSpec,
        volume_basename: &str,
        new_meta: &DestMeta,
    ) -> BR<()> {
        debug!(
            volume = %volume_basename,
            dest = %dest_spec.display_location(),
            archives = new_meta.archives().len(),
            "writing metadata"
        );
        write_dest::write_metadata_to_dest(dest_spec, volume_basename, new_meta)
    }

    fn write_subvolume(
        &self,
        dest_spec: &DestSpec,
        volume_basename: &str,
        snapshot: &str,
        max_chunk_size_mib: u64,
        chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<(Vec<ChunkFilename>, TransferStats)> {
        debug!(
            volume = %volume_basename,
            snapshot = %snapshot,
            dest = %dest_spec.display_location(),
            max_chunk_size_mib,
            "writing subvolume"
        );
        write_dest::write_subvolume_to_dest(
            dest_spec,
            volume_basename,
            snapshot,
            max_chunk_size_mib,
            chunks,
        )
    }

    fn measure_subvolume(
        &self,
        chunks: Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BR<measure::MeasureResult> {
        debug!("measuring subvolume stream");
        measure::measure_subvolume(chunks)
    }

    fn restore_archive(
        &self,
        dest_spec: &DestSpec,
        volume: &str,
        archive: &VolumeArchive,
        receive_root: &str,
    ) -> BR<()> {
        debug!(
            volume = %volume,
            snapshot = %archive.timestamp.raw(),
            chunks = archive.chunks.len(),
            root = %receive_root,
            "restoring archive"
        );

        // Read compressed chunks from dest as a single stream
        let chunk_reader = ChunkStreamReader::new(dest_spec, volume, archive)?;

        // Decompress
        let mut decoder = zstd::stream::read::Decoder::new(chunk_reader)?;

        // Spawn btrfs receive
        let mut child = self.btrfs_receive_command(receive_root)?;
        pipe_restore_stream(&mut decoder, &mut child)
    }
}

fn pipe_restore_stream(input: &mut impl Read, child: &mut std::process::Child) -> BR<()> {
    let mut stdin = child.stdin.take().unwrap();

    match std::io::copy(input, &mut stdin) {
        Ok(bytes) => {
            debug!(bytes, "wrote decompressed stream to btrfs receive");
        }
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            debug!("btrfs receive closed stdin early");
        }
        Err(e) => return Err(BbkarError::Io(e)),
    }
    drop(stdin);

    let mut stderr_str = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_str);
    }

    let status = child.wait().map_err(BbkarError::Io)?;
    if !status.success() {
        return Err(BbkarError::Execution(format!(
            "btrfs receive failed (exit {}): {}",
            status.code().unwrap_or(-1),
            stderr_str.trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::executor::sudo::ExecutionStrategy;
    use std::ffi::OsStr;
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Stdio;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> &'static Mutex<()> {
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn write_executable(path: &Path, script: &str) {
        std::fs::write(path, script).unwrap();
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    fn with_fake_path<T>(make_bin_dir: impl FnOnce(&Path), f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        make_bin_dir(bin_dir.path());
        let old_path = std::env::var_os("PATH");
        let new_path = match &old_path {
            Some(existing) => {
                let mut joined = std::ffi::OsString::from(bin_dir.path());
                joined.push(":");
                joined.push(existing);
                joined
            }
            None => std::ffi::OsString::from(bin_dir.path()),
        };

        // Test-only PATH override to route spawned btrfs/sudo commands to local shims.
        unsafe {
            std::env::set_var("PATH", &new_path);
        }
        let result = f();
        unsafe {
            match old_path {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
        }
        result
    }

    fn next_chunk(
        iter: &mut Box<dyn Iterator<Item = BR<BtrfsSendChunk>>>,
    ) -> BtrfsSendChunk {
        iter.next().unwrap().unwrap()
    }

    #[test]
    fn test_build_btrfs_receive_command_without_sudo() {
        let command = build_btrfs_receive_command("/root", false);

        assert_eq!(command.get_program(), OsStr::new("btrfs"));
        assert_eq!(args_of(&command), vec!["receive", "/root"]);
    }

    #[test]
    fn test_build_btrfs_receive_command_with_sudo() {
        let command = build_btrfs_receive_command("/root", true);

        assert_eq!(command.get_program(), OsStr::new("sudo"));
        assert_eq!(args_of(&command), vec!["btrfs", "receive", "/root"]);
    }

    #[test]
    fn test_build_btrfs_send_command_without_sudo() {
        let command = build_btrfs_send_command(&["-p", "/parent", "/snap"], false);

        assert_eq!(command.get_program(), OsStr::new("btrfs"));
        assert_eq!(args_of(&command), vec!["send", "-p", "/parent", "/snap"]);
    }

    #[test]
    fn test_build_btrfs_send_command_with_sudo() {
        let command = build_btrfs_send_command(&["/snap"], true);

        assert_eq!(command.get_program(), OsStr::new("sudo"));
        assert_eq!(args_of(&command), vec!["btrfs", "send", "/snap"]);
    }

    #[test]
    fn test_ensure_sudo_creates_session_when_missing() {
        let executor = RealExecutor::new().unwrap();

        executor
            .ensure_sudo_with(
                || {
                    Ok(SudoSession::test_session(ExecutionStrategy::Direct))
                },
                |_| Ok(()),
            )
            .unwrap();

        assert!(executor.sudo_session.borrow().is_some());
    }

    #[test]
    fn test_ensure_sudo_refreshes_existing_session() {
        let executor = RealExecutor {
            sudo_session: RefCell::new(Some(SudoSession::test_session(
                ExecutionStrategy::SudoPasswordless,
            ))),
        };
        let mut refreshed = false;

        executor
            .ensure_sudo_with(
                || unreachable!(),
                |_| {
                    refreshed = true;
                    Ok(())
                },
            )
            .unwrap();

        assert!(refreshed);
    }

    fn shell_child(script: &str) -> std::process::Child {
        Command::new("sh")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    #[test]
    fn test_pipe_restore_stream_success() {
        let payload = zstd::stream::encode_all(Cursor::new(b"restore payload"), 0).unwrap();
        let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(payload)).unwrap();
        let mut child = shell_child("cat >/dev/null");

        pipe_restore_stream(&mut decoder, &mut child).unwrap();
    }

    #[test]
    fn test_pipe_restore_stream_tolerates_broken_pipe() {
        let payload = zstd::stream::encode_all(Cursor::new(vec![b'a'; 1024]), 0).unwrap();
        let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(payload)).unwrap();
        let mut child = shell_child("exit 0");

        pipe_restore_stream(&mut decoder, &mut child).unwrap();
    }

    #[test]
    fn test_pipe_restore_stream_surfaces_child_failure() {
        let payload = zstd::stream::encode_all(Cursor::new(b"restore payload"), 0).unwrap();
        let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(payload)).unwrap();
        let mut child = shell_child("cat >/dev/null; echo bad >&2; exit 7");

        let err = pipe_restore_stream(&mut decoder, &mut child).unwrap_err();

        let rendered = format!("{err}");
        assert!(rendered.contains("btrfs receive failed (exit 7): bad"));
    }

    #[test]
    fn test_pipe_restore_stream_surfaces_input_io_error() {
        struct FailingReader;

        impl Read for FailingReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
        }

        let mut child = shell_child("cat >/dev/null");
        let err = pipe_restore_stream(&mut FailingReader, &mut child).unwrap_err();
        assert!(matches!(err, BbkarError::Io(_)));
    }

    #[test]
    fn test_read_subvolume_full_yields_stream_from_fake_btrfs() {
        with_fake_path(
            |bin_dir| {
                write_executable(
                    &bin_dir.join("btrfs"),
                    "#!/bin/sh\nprintf 'full-stream'\n",
                );
            },
            || {
                let executor = RealExecutor {
                    sudo_session: RefCell::new(Some(SudoSession::test_session(
                        ExecutionStrategy::Direct,
                    ))),
                };
                let mut iter = executor.read_subvolume_full(
                    SourceSpec {
                        path: "/source".into(),
                        filter: vec!["*".into()],
                    },
                    "snap-1",
                );

                match next_chunk(&mut iter) {
                    BtrfsSendChunk::StdoutBytes(bytes, offset) => {
                        assert_eq!(bytes, b"full-stream");
                        assert_eq!(offset, 0);
                    }
                    _ => panic!("unexpected first chunk kind"),
                }
                match next_chunk(&mut iter) {
                    BtrfsSendChunk::ProcessExit(code, stderr) => {
                        assert_eq!(code, 0);
                        assert_eq!(stderr, "");
                    }
                    _ => panic!("unexpected exit chunk kind"),
                }
            },
        );
    }

    #[test]
    fn test_read_subvolume_incremental_passes_parent_and_snapshot_to_fake_btrfs() {
        with_fake_path(
            |bin_dir| {
                write_executable(
                    &bin_dir.join("btrfs"),
                    "#!/bin/sh\nprintf '%s|%s|%s' \"$1\" \"$2\" \"$3\"\n",
                );
            },
            || {
                let executor = RealExecutor {
                    sudo_session: RefCell::new(Some(SudoSession::test_session(
                        ExecutionStrategy::Direct,
                    ))),
                };
                let mut iter = executor.read_subvolume_incremental(
                    SourceSpec {
                        path: "/source".into(),
                        filter: vec!["*".into()],
                    },
                    "snap-2",
                    "snap-1",
                );

                match next_chunk(&mut iter) {
                    BtrfsSendChunk::StdoutBytes(bytes, _) => {
                        assert_eq!(bytes, b"send|-p|/source/snap-1");
                    }
                    _ => panic!("unexpected first chunk kind"),
                }
            },
        );
    }

    #[test]
    fn test_restore_archive_restores_via_fake_btrfs_receive() {
        with_fake_path(
            |bin_dir| {
                write_executable(
                    &bin_dir.join("btrfs"),
                    "#!/bin/sh\nif [ \"$1\" = \"receive\" ]; then cat > \"$2/restored.bin\"; else exit 99; fi\n",
                );
            },
            || {
                let tmp = tempfile::tempdir().unwrap();
                let dest_root = tmp.path().join("dest");
                let receive_root = tmp.path().join("recv");
                std::fs::create_dir_all(&receive_root).unwrap();
                let dest_spec = DestSpec {
                    backend_spec: crate::model::config::BackendSpec::Local {
                        path: dest_root.to_string_lossy().into_owned(),
                    },
                };
                let payload = zstd::stream::encode_all(Cursor::new(b"restored payload"), 0).unwrap();
                let snapshot_dir = dest_root.join("vol").join("20230101");
                std::fs::create_dir_all(&snapshot_dir).unwrap();
                std::fs::write(snapshot_dir.join("part000001.btrfs.zstd"), payload).unwrap();
                let archive = VolumeArchive {
                    timestamp: crate::model::source::Timestamp::parse("20230101").unwrap(),
                    parent_timestamp: None,
                    chunks: vec![ChunkFilename::new(
                        "part000001.btrfs.zstd".into(),
                        snapshot_dir
                            .join("part000001.btrfs.zstd")
                            .metadata()
                            .unwrap()
                            .len() as u32,
                        Some("zstd".into()),
                        Some(16),
                        Some("deadbeef".into()),
                    )],
                };
                let executor = RealExecutor {
                    sudo_session: RefCell::new(Some(SudoSession::test_session(
                        ExecutionStrategy::Direct,
                    ))),
                };

                executor
                    .restore_archive(
                        &dest_spec,
                        "vol",
                        &archive,
                        receive_root.to_str().unwrap(),
                    )
                    .unwrap();

                let restored = std::fs::read(receive_root.join("restored.bin")).unwrap();
                assert_eq!(restored, b"restored payload");
            },
        );
    }
}

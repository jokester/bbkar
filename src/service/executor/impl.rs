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
    use std::process::Stdio;

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
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
}

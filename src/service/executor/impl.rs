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
        let mut session = self.sudo_session.borrow_mut();
        if session.is_none() {
            debug!("initializing sudo session");
            *session = Some(SudoSession::new()?);
        } else {
            debug!("refreshing sudo session");
            session.as_mut().unwrap().ensure_active()?;
        }
        Ok(())
    }

    fn btrfs_receive_command(&self, root: &str) -> BR<std::process::Child> {
        self.ensure_sudo()?;
        let needs_sudo = self.sudo_session.borrow().as_ref().unwrap().needs_sudo();
        debug!(root = %root, needs_sudo, "spawning btrfs receive");

        let child = if needs_sudo {
            Command::new("sudo")
                .args(["btrfs", "receive", root])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            Command::new("btrfs")
                .args(["receive", root])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
        };
        child.map_err(BbkarError::Io)
    }

    fn btrfs_send_command(&self, args: &[&str]) -> BR<std::process::Child> {
        self.ensure_sudo()?;
        let needs_sudo = self.sudo_session.borrow().as_ref().unwrap().needs_sudo();
        debug!(args = ?args, needs_sudo, "spawning btrfs send");

        let child = if needs_sudo {
            let mut sudo_args = vec!["btrfs", "send"];
            sudo_args.extend_from_slice(args);
            Command::new("sudo")
                .args(&sudo_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            Command::new("btrfs")
                .arg("send")
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };
        child.map_err(BbkarError::Io)
    }
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
        let mut stdin = child.stdin.take().unwrap();

        // Pipe decompressed stream to btrfs receive stdin
        match std::io::copy(&mut decoder, &mut stdin) {
            Ok(bytes) => {
                debug!(bytes, "wrote decompressed stream to btrfs receive");
            }
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                debug!("btrfs receive closed stdin early");
            }
            Err(e) => return Err(BbkarError::Io(e)),
        }
        drop(stdin);

        // Read stderr and wait for exit
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
}

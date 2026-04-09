use std::path::Path;

use crate::model::error::BR;
use crate::service::executor::Executor;

pub fn status(config_path: &Path, name: Option<&str>, executor: Box<dyn Executor>) -> BR<()> {
    executor.print_info("bbkar status");

    super::for_each_volume(config_path, &*executor, name, |ctx| {
        executor.print_info(&format!(
            "    {} local snapshots: {} - {}",
            ctx.src_state.volume.snapshots().len(),
            ctx.src_state.volume.oldest_snapshot().raw(),
            ctx.src_state.volume.newest_snapshot().raw(),
        ));
        let dest_state = executor.inspect_dest_volume(ctx.dest_spec, ctx.volume)?;
        executor.print_info(&format!(
            "    remote range (see `bbkar ls` for details): {} - {}",
            dest_state
                .meta
                .as_ref()
                .map(|m| m
                    .oldest_archive()
                    .map(|a| a.timestamp.raw())
                    .unwrap_or_default())
                .unwrap_or("None"),
            dest_state
                .meta
                .as_ref()
                .map(|m| m
                    .newest_archive()
                    .map(|a| a.timestamp.raw())
                    .unwrap_or_default())
                .unwrap_or("None"),
        ));
        super::print_archive_stats(
            &*executor,
            "    ",
            "remote usage",
            super::ArchiveStats::from_meta(dest_state.meta.as_ref()),
        );
        Ok(())
    })
}

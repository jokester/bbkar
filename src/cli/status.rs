use std::path::Path;

use crate::model::error::BR;
use crate::service::executor::Executor;
use crate::service::planner::Planner;

pub fn status(config_path: &Path, name: Option<&str>, executor: Box<dyn Executor>) -> BR<()> {
    let planner = Planner;
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
        executor.print_info(&format!(
            "    retention: {}",
            ctx.retention_policy.describe()
        ));
        let prune_plan = planner.build_prune_plan(dest_state.meta.as_ref(), &ctx.retention_policy);
        executor.print_info(&format!(
            "    next prune: keep {} archive(s), prune {} archive(s), required ancestor {} archive(s)",
            prune_plan.kept_count(),
            prune_plan.pruned_count(),
            prune_plan.required_ancestor_count(),
        ));
        Ok(())
    })
}

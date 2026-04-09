use std::cell::RefCell;
use std::path::Path;

use crate::cli::Commands;
use crate::model::error::BR;
use crate::model::plan::RunStep;
use crate::service::executor::Executor;
use crate::service::planner::Planner;
use crate::utils::format::format_bytes;

pub fn dryrun(config_path: &Path, cmd: Commands, executor: Box<dyn Executor>) -> BR<()> {
    let Commands::Dryrun {
        measure_send, name, ..
    } = cmd
    else {
        unreachable!()
    };
    let planner = Planner;
    let sync_would_send_stats = RefCell::new(super::ArchiveStats::default());
    let sync_remote_stats = RefCell::new(super::ArchiveStats::default());

    executor.print_info("bbkar dryrun");

    super::for_each_sync(
        config_path,
        &*executor,
        name.as_deref(),
        |_, _, _, _| {
            *sync_would_send_stats.borrow_mut() = super::ArchiveStats::default();
            *sync_remote_stats.borrow_mut() = super::ArchiveStats::default();
            Ok(())
        },
        |ctx| {
            let dest_state = executor.inspect_dest_volume(ctx.dest_spec, ctx.volume)?;
            sync_remote_stats
                .borrow_mut()
                .merge(super::ArchiveStats::from_meta(dest_state.meta.as_ref()));
            let plan = planner.build_plan(
                ctx.volume,
                ctx.src_state,
                ctx.dest_spec,
                &dest_state,
                &ctx.send_policy,
            )?;

            if plan.steps.is_empty() {
                executor.print_info("    (up to date)");
                return Ok(());
            }
            for step in plan.steps {
                match step {
                    RunStep::SendFull(snap) => {
                        if measure_send {
                            let subvolume = format!("{}.{}", ctx.volume, snap.raw());
                            let chunks =
                                executor.read_subvolume_full(ctx.src_spec.clone(), &subvolume);
                            let result = executor.measure_subvolume(chunks)?;
                            sync_would_send_stats
                                .borrow_mut()
                                .add_size(false, result.compressed);
                            sync_remote_stats
                                .borrow_mut()
                                .add_size(false, result.compressed);
                            executor.print_info(&format!(
                                "    would send full: {} ({} compressed, {} raw)",
                                snap.raw(),
                                format_bytes(result.compressed),
                                format_bytes(result.uncompressed)
                            ));
                        } else {
                            executor.print_info(&format!("    would send full: {}", snap.raw()));
                        }
                    }
                    RunStep::SendIncremental(snap, parent) => {
                        if measure_send {
                            let subvolume = format!("{}.{}", ctx.volume, snap.raw());
                            let parent_subvolume = format!("{}.{}", ctx.volume, parent.raw());
                            let chunks = executor.read_subvolume_incremental(
                                ctx.src_spec.clone(),
                                &subvolume,
                                &parent_subvolume,
                            );
                            let result = executor.measure_subvolume(chunks)?;
                            sync_would_send_stats
                                .borrow_mut()
                                .add_size(true, result.compressed);
                            sync_remote_stats
                                .borrow_mut()
                                .add_size(true, result.compressed);
                            executor.print_info(&format!(
                                "    would send incremental: {} (parent: {}) ({} compressed, {} raw)",
                                snap.raw(),
                                parent.raw(),
                                format_bytes(result.compressed),
                                format_bytes(result.uncompressed)
                            ));
                        } else {
                            executor.print_info(&format!(
                                "    would send incremental: {} (parent: {})",
                                snap.raw(),
                                parent.raw()
                            ));
                        }
                    }
                }
            }
            Ok(())
        },
        |_, _, _, _| {
            if measure_send {
                super::print_archive_stats(
                    &*executor,
                    "  ",
                    "sync would send",
                    *sync_would_send_stats.borrow(),
                );
            }
            super::print_archive_stats(
                &*executor,
                "  ",
                "sync remote usage",
                *sync_remote_stats.borrow(),
            );
            Ok(())
        },
    )
}

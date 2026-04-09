use std::cell::RefCell;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::dest::{DestMeta, VolumeArchive};
use crate::model::error::BR;
use crate::model::plan::RunStep;
use crate::service::executor::Executor;
use crate::service::planner::Planner;
use crate::utils::format::format_bytes;

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn run(config_path: &Path, name: Option<&str>, executor: Box<dyn Executor>) -> BR<()> {
    let planner = Planner;
    let sync_sent_stats = RefCell::new(super::ArchiveStats::default());
    let sync_remote_stats = RefCell::new(super::ArchiveStats::default());
    let total_bytes_sent = RefCell::new(0u64);

    executor.print_info("bbkar run");

    super::for_each_sync(
        config_path,
        &*executor,
        name,
        |_, _, _, _| {
            *sync_sent_stats.borrow_mut() = super::ArchiveStats::default();
            *sync_remote_stats.borrow_mut() = super::ArchiveStats::default();
            Ok(())
        },
        |ctx| {
            let dest_state = executor.inspect_dest_volume(ctx.dest_spec, ctx.volume)?;
            let plan = planner.build_plan(
                ctx.volume,
                ctx.src_state,
                ctx.dest_spec,
                &dest_state,
                &ctx.send_policy,
            )?;

            if plan.steps.is_empty() {
                executor.print_info("    (up to date)");
                sync_remote_stats
                    .borrow_mut()
                    .merge(super::ArchiveStats::from_meta(dest_state.meta.as_ref()));
                return Ok(());
            }

            let dest_location = ctx.dest_spec.display_location();
            let mut dest_meta = dest_state
                .meta
                .unwrap_or_else(|| DestMeta::new(now_timestamp(), 0, vec![]));

            for step in plan.steps {
                match step {
                    RunStep::SendFull(snap) => {
                        executor.print_info(&format!("    sending full: {}", snap.raw()));
                        let subvolume = format!("{}.{}", ctx.volume, snap.raw());
                        let chunks = executor.read_subvolume_full(ctx.src_spec.clone(), &subvolume);
                        let (chunk_files, stats) = executor.write_subvolume(
                            ctx.dest_spec,
                            ctx.volume,
                            snap.raw(),
                            ctx.global.max_backup_chunk_size,
                            chunks,
                        )?;

                        let archive_dir =
                            format!("{}/{}/{}/", dest_location, ctx.volume, snap.raw());
                        executor.print_info(&format!(
                            "    created archive dir: {} ({} chunk(s))",
                            archive_dir,
                            chunk_files.len()
                        ));

                        let archive = VolumeArchive {
                            timestamp: snap.clone(),
                            parent_timestamp: None,
                            chunks: chunk_files,
                        };
                        let total = archive.total_size();
                        let raw_total = archive.total_raw_size().unwrap_or(total);
                        *total_bytes_sent.borrow_mut() += total;
                        sync_sent_stats.borrow_mut().add_archive(&archive);
                        dest_meta.add_archive(archive);
                        dest_meta.set_last_sync_timestamp(now_timestamp());
                        executor.write_metadata(ctx.dest_spec, ctx.volume, &dest_meta)?;

                        executor.print_info(&format!(
                            "    metadata updated: {}/{}/",
                            dest_location, ctx.volume
                        ));
                        executor.print_info(&format!(
                            "    done: {} (sent {} compressed, {} raw) {:.1}s",
                            snap.raw(),
                            format_bytes(total),
                            format_bytes(raw_total),
                            stats.elapsed.as_secs_f64()
                        ));
                    }
                    RunStep::SendIncremental(snap, parent) => {
                        executor.print_info(&format!(
                            "    sending incremental: {} (parent: {})",
                            snap.raw(),
                            parent.raw()
                        ));
                        let subvolume = format!("{}.{}", ctx.volume, snap.raw());
                        let parent_subvolume = format!("{}.{}", ctx.volume, parent.raw());
                        let chunks = executor.read_subvolume_incremental(
                            ctx.src_spec.clone(),
                            &subvolume,
                            &parent_subvolume,
                        );
                        let (chunk_files, stats) = executor.write_subvolume(
                            ctx.dest_spec,
                            ctx.volume,
                            snap.raw(),
                            ctx.global.max_backup_chunk_size,
                            chunks,
                        )?;

                        let archive_dir =
                            format!("{}/{}/{}/", dest_location, ctx.volume, snap.raw());
                        executor.print_info(&format!(
                            "    created archive dir: {} ({} chunk(s))",
                            archive_dir,
                            chunk_files.len()
                        ));

                        let archive = VolumeArchive {
                            timestamp: snap.clone(),
                            parent_timestamp: Some(parent.raw().to_string()),
                            chunks: chunk_files,
                        };
                        let total = archive.total_size();
                        let raw_total = archive.total_raw_size().unwrap_or(total);
                        *total_bytes_sent.borrow_mut() += total;
                        sync_sent_stats.borrow_mut().add_archive(&archive);
                        dest_meta.add_archive(archive);
                        dest_meta.set_last_sync_timestamp(now_timestamp());
                        executor.write_metadata(ctx.dest_spec, ctx.volume, &dest_meta)?;

                        executor.print_info(&format!(
                            "    metadata updated: {}/{}/",
                            dest_location, ctx.volume
                        ));
                        executor.print_info(&format!(
                            "    done: {} (sent {} compressed, {} raw) {:.1}s",
                            snap.raw(),
                            format_bytes(total),
                            format_bytes(raw_total),
                            stats.elapsed.as_secs_f64()
                        ));
                    }
                }
            }

            sync_remote_stats
                .borrow_mut()
                .merge(super::ArchiveStats::from_meta(Some(&dest_meta)));
            Ok(())
        },
        |_, _, _, _| {
            super::print_archive_stats(&*executor, "  ", "sync sent", *sync_sent_stats.borrow());
            super::print_archive_stats(
                &*executor,
                "  ",
                "sync remote usage",
                *sync_remote_stats.borrow(),
            );
            Ok(())
        },
    )?;

    executor.print_info(&format!(
        "total bytes sent: {}",
        format_bytes(*total_bytes_sent.borrow())
    ));

    Ok(())
}

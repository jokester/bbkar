use std::collections::BTreeSet;
use std::path::Path;

use crate::model::dest::DestState;
use crate::model::error::{BR, BbkarError};
use crate::model::plan::PruneDecision;
use crate::model::source::Timestamp;
use crate::service::executor::Executor;
use crate::service::planner::Planner;

pub fn ls(
    config_path: &Path,
    name: Option<&str>,
    restore_root: Option<&str>,
    executor: Box<dyn Executor>,
) -> BR<()> {
    let planner = Planner;
    executor.print_info("bbkar ls");

    if let Some(root) = restore_root {
        let resolved_name = resolve_sync_name(config_path, name)?;
        super::for_each_volume(config_path, &*executor, Some(&resolved_name), |ctx| {
            let dest_state = executor.inspect_dest_volume(ctx.dest_spec, ctx.volume)?;
            print_restore_table(&*executor, ctx.volume, &dest_state, root);
            Ok(())
        })
    } else {
        super::for_each_volume(config_path, &*executor, name, |ctx| {
            let dest_state = executor.inspect_dest_volume(ctx.dest_spec, ctx.volume)?;
            let prune_plan = planner.build_prune_plan(dest_state.meta.as_ref(), &ctx.retention_policy);
            print_snapshot_table(
                &*executor,
                ctx.volume,
                &dest_state,
                ctx.src_state.volume.snapshots(),
                &prune_plan.decisions,
            );
            Ok(())
        })
    }
}

/// Mandate --name when multiple [sync] blocks exist, auto-select when only one.
fn resolve_sync_name(config_path: &Path, name: Option<&str>) -> BR<String> {
    let config = super::load_config(config_path)?;
    match name {
        Some(n) => {
            if !config.sync.contains_key(n) {
                let names: Vec<&str> = config.sync.keys().map(|s| s.as_str()).collect();
                return Err(BbkarError::Config(vec![format!(
                    "sync '{}' not found in config (available: {})",
                    n,
                    names.join(", ")
                )]));
            }
            Ok(n.to_string())
        }
        None => {
            if config.sync.len() > 1 {
                let names: Vec<&str> = config.sync.keys().map(|s| s.as_str()).collect();
                return Err(BbkarError::Config(vec![format!(
                    "multiple syncs defined ({}), use --name to select one",
                    names.join(", ")
                )]));
            }
            config
                .sync
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| BbkarError::Config(vec!["no sync defined in config".into()]))
        }
    }
}

fn print_snapshot_table(
    executor: &dyn Executor,
    volume: &str,
    dest_state: &DestState,
    local_snapshots: &[Timestamp],
    prune_decisions: &[PruneDecision],
) {
    let local: BTreeSet<Timestamp> = local_snapshots.iter().cloned().collect();
    let remote: BTreeSet<Timestamp> = dest_state
        .meta
        .as_ref()
        .map(|meta| {
            meta.archives()
                .iter()
                .map(|archive| archive.timestamp.clone())
                .collect()
        })
        .unwrap_or_default();
    let prune_map = prune_decisions
        .iter()
        .map(|decision| (decision.snapshot.raw(), decision.prune_status()))
        .collect::<std::collections::HashMap<_, _>>();

    // Column width: volume name + "." + longest snapshot name
    let max_snap_len = local
        .iter()
        .chain(remote.iter())
        .map(|s| s.raw().len())
        .max()
        .unwrap_or(0);
    let col_width = volume.len() + 1 + max_snap_len;
    let prune_width = "keep(required)".len();

    executor.print_info(&format!(
        "    {:<col_width$} {:<11} {:<prune_width$}",
        "snapshot", "state", "prune"
    ));
    for snapshot in local.union(&remote) {
        let state = match (local.contains(snapshot), remote.contains(snapshot)) {
            (true, false) => "local-only",
            (false, true) => "remote-only",
            (true, true) => "synced",
            (false, false) => unreachable!("union membership mismatch"),
        };
        let prune = if remote.contains(snapshot) {
            prune_map.get(snapshot.raw()).copied().unwrap_or("keep")
        } else {
            "-"
        };
        let name = format!("{}.{}", volume, snapshot.raw());
        executor.print_info(&format!(
            "    {:<col_width$} {:<11} {:<prune_width$}",
            name, state, prune
        ));
    }
}

fn print_restore_table(
    executor: &dyn Executor,
    volume: &str,
    dest_state: &DestState,
    restore_root: &str,
) {
    let archived: BTreeSet<Timestamp> = dest_state
        .meta
        .as_ref()
        .map(|meta| {
            meta.archives()
                .iter()
                .map(|archive| archive.timestamp.clone())
                .collect()
        })
        .unwrap_or_default();

    // Scan restore root for subvolumes matching "{volume}.{timestamp}"
    let prefix = format!("{}.", volume);
    let mut restored: BTreeSet<String> = BTreeSet::new();
    let root_path = Path::new(restore_root);
    if root_path.is_dir() {
        for entry in std::fs::read_dir(root_path).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) {
                restored.insert(name[prefix.len()..].to_string());
            }
        }
    }

    // Collect all timestamps from both sets
    let archived_raws: BTreeSet<&str> = archived.iter().map(|t| t.raw()).collect();
    let restored_raws: BTreeSet<&str> = restored.iter().map(|s| s.as_str()).collect();
    let all_raws: BTreeSet<&str> = archived_raws.union(&restored_raws).copied().collect();

    let max_snap_len = all_raws.iter().map(|s| s.len()).max().unwrap_or(0);
    let col_width = volume.len() + 1 + max_snap_len;

    executor.print_info(&format!("    {:<col_width$} state", "snapshot"));
    for raw in &all_raws {
        let in_archive = archived_raws.contains(raw);
        let in_restored = restored_raws.contains(raw);
        let state = match (in_archive, in_restored) {
            (true, false) => "not-restored",
            (true, true) => "restored",
            (false, true) => "restored-only",
            (false, false) => unreachable!(),
        };
        let name = format!("{}.{}", volume, raw);
        executor.print_info(&format!("    {:<col_width$} {}", name, state));
    }
}

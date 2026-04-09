use std::collections::HashSet;
use std::path::Path;

use tracing::info;

use crate::model::config::DestSpec;
use crate::model::dest::DestMeta;
use crate::model::error::{BR, BbkarError};
use crate::model::plan::RestoreStep;
use crate::model::source::Timestamp;
use crate::service::executor::Executor;
use crate::service::planner::Planner;

/// Shared state produced by prepare_restore, consumed by restore/dryrestore.
struct RestorePlan<'a> {
    dest_spec: &'a DestSpec,
    volume: &'a str,
    meta: DestMeta,
    steps: Vec<RestoreStep>,
    pre_existing: HashSet<String>,
}

#[allow(clippy::too_many_arguments)]
fn prepare_restore<'a>(
    config_path: &Path,
    root: &str,
    volume: &'a str,
    snapshots: &[String],
    min_timestamp: Option<&str>,
    max_timestamp: Option<&str>,
    name: Option<&str>,
    executor: &dyn Executor,
    config_out: &'a mut Option<crate::model::config::BbkarConfigFile>,
) -> BR<RestorePlan<'a>> {
    let config = super::load_config(config_path)?;
    *config_out = Some(config);
    let config = config_out.as_ref().unwrap();

    // Resolve the sync block
    let (_sync_name, sync_spec) = match name {
        Some(n) => {
            let spec = config.sync.get(n).ok_or_else(|| {
                let names: Vec<&str> = config.sync.keys().map(|s| s.as_str()).collect();
                BbkarError::Config(vec![format!(
                    "sync '{}' not found in config (available: {})",
                    n,
                    names.join(", ")
                )])
            })?;
            info!(sync = %n, "using sync (--name)");
            (n.to_string(), spec)
        }
        None => {
            if config.sync.len() > 1 {
                let names: Vec<&str> = config.sync.keys().map(|s| s.as_str()).collect();
                return Err(BbkarError::Config(vec![format!(
                    "multiple syncs defined ({}), use --name to select one",
                    names.join(", ")
                )]));
            }
            let (k, v) = config
                .sync
                .iter()
                .next()
                .ok_or_else(|| BbkarError::Config(vec!["no sync defined in config".into()]))?;
            executor.print_info(&format!("  assuming --name={} as only 1 [sync] exists", k));
            (k.clone(), v)
        }
    };
    let dest_spec = config.dest.get(&sync_spec.dest).ok_or_else(|| {
        BbkarError::Config(vec![format!(
            "dest '{}' referenced by sync not found",
            sync_spec.dest
        )])
    })?;

    executor.print_info(&format!(
        "  source: {}/{}",
        dest_spec.display_location(),
        volume
    ));
    executor.print_info(&format!("  target: {}", root));

    // Read destination metadata for this volume
    let dest_state = executor.inspect_dest_volume(dest_spec, volume)?;
    let meta = dest_state.meta.ok_or_else(|| {
        BbkarError::Execution(format!("no archives found for volume '{}'", volume))
    })?;

    // Determine target timestamps
    let targets = resolve_targets(meta.archives(), snapshots, min_timestamp, max_timestamp)?;

    for target in &targets {
        executor.print_info(&format!("  target timestamp: {}", target.raw()));
    }

    // Build merged restore plan: collect steps from all targets, deduplicate
    let planner = Planner;
    let mut all_steps: Vec<RestoreStep> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for target in &targets {
        let plan = planner.build_restore_plan(meta.archives(), target)?;
        for step in plan.steps {
            let ts_raw = match &step {
                RestoreStep::ReceiveFull(ts) => ts.raw().to_string(),
                RestoreStep::ReceiveIncremental(ts, _) => ts.raw().to_string(),
            };
            if seen.insert(ts_raw) {
                all_steps.push(step);
            }
        }
    }

    // Sort by timestamp to ensure correct dependency order
    // (parents are always older, so chronological order = valid apply order)
    all_steps.sort_by(|a, b| {
        let ts_a = match a {
            RestoreStep::ReceiveFull(ts) | RestoreStep::ReceiveIncremental(ts, _) => ts,
        };
        let ts_b = match b {
            RestoreStep::ReceiveFull(ts) | RestoreStep::ReceiveIncremental(ts, _) => ts,
        };
        ts_a.cmp(ts_b)
    });

    executor.print_info(&format!("  restore chain: {} step(s)", all_steps.len()));
    for step in &all_steps {
        match step {
            RestoreStep::ReceiveFull(snap) => {
                executor.print_info(&format!("    {} (full)", snap.raw()));
            }
            RestoreStep::ReceiveIncremental(snap, parent) => {
                executor.print_info(&format!(
                    "    {} (incremental, parent: {})",
                    snap.raw(),
                    parent.raw()
                ));
            }
        }
    }

    // Scan existing subvolumes in --root to skip already-restored snapshots
    let mut pre_existing: HashSet<String> = HashSet::new();
    let root_path = Path::new(root);
    if root_path.is_dir() {
        for entry in std::fs::read_dir(root_path)?.flatten() {
            pre_existing.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }

    Ok(RestorePlan {
        dest_spec,
        volume,
        meta,
        steps: all_steps,
        pre_existing,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn restore(
    config_path: &Path,
    root: &str,
    volume: &str,
    snapshots: &[String],
    min_timestamp: Option<&str>,
    max_timestamp: Option<&str>,
    name: Option<&str>,
    executor: Box<dyn Executor>,
) -> BR<()> {
    executor.print_info("bbkar restore");

    let mut config = None;
    let plan = prepare_restore(
        config_path,
        root,
        volume,
        snapshots,
        min_timestamp,
        max_timestamp,
        name,
        &*executor,
        &mut config,
    )?;

    let mut received: HashSet<String> = HashSet::new();

    for step in &plan.steps {
        let (snap_name, kind) = match step {
            RestoreStep::ReceiveFull(snap) => (snap, "full"),
            RestoreStep::ReceiveIncremental(snap, _) => (snap, "incremental"),
        };

        let subvol_name = format!("{}.{}", plan.volume, snap_name.raw());
        if plan.pre_existing.contains(&subvol_name) {
            executor.print_warn(&format!(
                "  skipping {} (already exists in root before restore, assuming valid)",
                subvol_name
            ));
            continue;
        }
        if received.contains(&subvol_name) {
            executor.print_info(&format!(
                "  skipping {} (received earlier this run)",
                subvol_name
            ));
            continue;
        }

        executor.print_info(&format!("  receiving {} {}...", kind, snap_name.raw()));

        let archive = plan
            .meta
            .archives()
            .iter()
            .find(|a| a.timestamp == *snap_name)
            .unwrap();
        executor.restore_archive(plan.dest_spec, plan.volume, archive, root)?;
        received.insert(subvol_name);
        executor.print_info(&format!("  done: {}", snap_name.raw()));
    }

    executor.print_info("restore complete");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn dryrestore(
    config_path: &Path,
    root: &str,
    volume: &str,
    snapshots: &[String],
    min_timestamp: Option<&str>,
    max_timestamp: Option<&str>,
    name: Option<&str>,
    executor: Box<dyn Executor>,
) -> BR<()> {
    executor.print_info("bbkar dryrestore");

    let mut config = None;
    let plan = prepare_restore(
        config_path,
        root,
        volume,
        snapshots,
        min_timestamp,
        max_timestamp,
        name,
        &*executor,
        &mut config,
    )?;

    for step in &plan.steps {
        let (snap_name, kind) = match step {
            RestoreStep::ReceiveFull(snap) => (snap, "full"),
            RestoreStep::ReceiveIncremental(snap, _) => (snap, "incremental"),
        };

        let subvol_name = format!("{}.{}", plan.volume, snap_name.raw());
        if plan.pre_existing.contains(&subvol_name) {
            executor.print_info(&format!(
                "  would skip {} (already exists in root)",
                subvol_name
            ));
        } else {
            executor.print_info(&format!("  would receive {} {}", kind, snap_name.raw()));
        }
    }

    executor.print_info("dryrestore complete");
    Ok(())
}

/// Resolve target timestamps from CLI args.
/// - If explicit snapshots given: use those exact timestamps
/// - If min/max range given: include all archives in range
/// - If nothing given: default to latest
fn resolve_targets(
    archives: &[crate::model::dest::VolumeArchive],
    snapshots: &[String],
    min_timestamp: Option<&str>,
    max_timestamp: Option<&str>,
) -> BR<Vec<Timestamp>> {
    let has_explicit = !snapshots.is_empty();
    let has_range = min_timestamp.is_some() || max_timestamp.is_some();

    if !has_explicit && !has_range {
        // Default: latest
        let newest = archives
            .iter()
            .max_by_key(|a| &a.timestamp)
            .ok_or_else(|| BbkarError::Execution("no archives found".into()))?;
        return Ok(vec![newest.timestamp.clone()]);
    }

    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    // Add explicit snapshot targets
    for snap_raw in snapshots {
        let archive = archives
            .iter()
            .find(|a| a.timestamp.raw() == snap_raw.as_str())
            .ok_or_else(|| {
                BbkarError::Execution(format!("snapshot '{}' not found in archives", snap_raw))
            })?;
        if seen.insert(archive.timestamp.raw().to_string()) {
            targets.push(archive.timestamp.clone());
        }
    }

    // Add archives matching timestamp range
    if has_range {
        for archive in archives {
            let raw = archive.timestamp.raw();
            if let Some(min) = min_timestamp
                && raw < min
            {
                continue;
            }
            if let Some(max) = max_timestamp
                && raw > max
            {
                continue;
            }
            if seen.insert(raw.to_string()) {
                targets.push(archive.timestamp.clone());
            }
        }
    }

    if targets.is_empty() {
        return Err(BbkarError::Execution(
            "no archives match the given snapshot/timestamp filters".into(),
        ));
    }

    targets.sort();
    Ok(targets)
}

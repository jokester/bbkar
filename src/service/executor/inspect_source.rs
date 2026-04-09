use crate::model::config::SourceSpec;
use crate::model::error::{BR, BbkarError};
use crate::model::source::{Series, Timestamp};
use crate::utils::wildcard::wildcard_match;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, trace};

/**
 * State of a single volume within a source location.
 */
#[derive(Clone)]
pub struct SourceState {
    pub volume: Series,
}

/// Inspects a source directory and groups snapshot subvolumes by basename.
///
/// This function lists entries in `source_spec.path`, parses each as `BASENAME.TIMESTAMP[_ID]`,
/// groups them by basename, and filters by the given patterns.
///
/// For example, if the directory contains:
/// - root.20230101
/// - root.20230102
/// - root.20230102_2
/// - home.20230101
///
/// With subvolume = ["*"], the result will be:
/// - "root" -> SourceState with snapshots ["20230101", "20230102", "20230102_2"]
/// - "home" -> SourceState with snapshots ["20230101"]
pub fn inspect_source(source_spec: &SourceSpec) -> BR<HashMap<String, SourceState>> {
    let path = Path::new(&source_spec.path);
    debug!(path = %source_spec.path, filters = ?source_spec.filter, "scanning source directory");
    if !path.is_dir() {
        return Err(BbkarError::InvalidSourcePath(source_spec.path.clone()));
    }

    // List directory entries and parse as BASENAME.SUFFIX
    let mut grouped: HashMap<String, Vec<Timestamp>> = HashMap::new();
    let mut skipped_no_dot = 0u64;
    let mut skipped_bad_suffix = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Split on last '.' to get basename and snapshot suffix
        // this should be safe because btrbk suffixes contains no .
        let Some((basename, suffix)) = name.rsplit_once('.') else {
            skipped_no_dot += 1;
            continue;
        };
        let Some(snapshot) = Timestamp::parse(suffix) else {
            skipped_bad_suffix += 1;
            continue;
        };
        trace!(basename = %basename, snapshot = %snapshot.raw(), "found source snapshot");

        grouped
            .entry(basename.to_string())
            .or_default()
            .push(snapshot);
    }

    // Filter by patterns and build result
    let mut result = HashMap::new();
    let mut filtered_out = 0u64;
    for (basename, snapshots) in grouped {
        if !source_spec
            .filter
            .iter()
            .any(|p| wildcard_match(p, &basename))
        {
            filtered_out += 1;
            continue;
        }
        debug!(
            volume = %basename,
            snapshots = snapshots.len(),
            "source volume matched filters"
        );
        result.insert(
            basename.clone(),
            SourceState {
                volume: Series::new(basename, snapshots),
            },
        );
    }

    debug!(
        matched_volumes = result.len(),
        skipped_no_dot, skipped_bad_suffix, filtered_out, "source scan complete"
    );
    Ok(result)
}

#[cfg(test)]
mod test_inspect_source {
    use super::*;

    fn make_spec(dir: &std::path::Path) -> SourceSpec {
        SourceSpec {
            path: dir.to_string_lossy().to_string(),
            filter: vec!["*".to_string()],
        }
    }

    #[test]
    fn test_invalid_path() {
        let spec = SourceSpec {
            path: "/nonexistent/path".to_string(),
            filter: vec![],
        };
        let result = inspect_source(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let spec = make_spec(dir.path());
        let result = inspect_source(&spec).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_groups_by_basename() {
        let dir = tempfile::tempdir().unwrap();
        // Create snapshot dirs
        for name in &[
            "root.20230101",
            "root.20230102",
            "root.20230102_2",
            "home.20230101",
        ] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }

        let spec = make_spec(dir.path());
        let result = inspect_source(&spec).unwrap();

        assert_eq!(result.len(), 2);

        let root = &result["root"];
        assert_eq!(root.volume.orig_name(), "root");
        assert_eq!(root.volume.snapshots().len(), 3);
        assert_eq!(root.volume.snapshots()[0].raw(), "20230101");
        assert_eq!(root.volume.snapshots()[1].raw(), "20230102");
        assert_eq!(root.volume.snapshots()[2].raw(), "20230102_2");

        let home = &result["home"];
        assert_eq!(home.volume.orig_name(), "home");
        assert_eq!(home.volume.snapshots().len(), 1);
    }

    #[test]
    fn test_pattern_filtering() {
        let dir = tempfile::tempdir().unwrap();
        for name in &["root.20230101", "home.20230101", "var.20230101"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }

        // Exact match
        let spec = SourceSpec {
            path: dir.path().to_string_lossy().to_string(),
            filter: vec!["root".to_string()],
        };
        let result = inspect_source(&spec).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("root"));

        // Glob match
        let spec = SourceSpec {
            path: dir.path().to_string_lossy().to_string(),
            filter: vec!["h*".to_string()],
        };
        let result = inspect_source(&spec).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("home"));

        // Multiple patterns
        let spec = SourceSpec {
            path: dir.path().to_string_lossy().to_string(),
            filter: vec!["root".to_string(), "var".to_string()],
        };
        let result = inspect_source(&spec).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_skips_entries_without_dot() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nodot")).unwrap();
        std::fs::create_dir(dir.path().join("root.20230101")).unwrap();

        let spec = make_spec(dir.path());
        let result = inspect_source(&spec).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("root"));
    }

    #[test]
    fn test_basename_with_dots() {
        let dir = tempfile::tempdir().unwrap();
        for name in &[
            "@home.data.20230101",
            "@home.data.20230102",
            "my.server.root.20230101",
        ] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }

        let spec = make_spec(dir.path());
        let result = inspect_source(&spec).unwrap();

        assert_eq!(result.len(), 2);

        let home = &result["@home.data"];
        assert_eq!(home.volume.orig_name(), "@home.data");
        assert_eq!(home.volume.snapshots().len(), 2);
        assert_eq!(home.volume.snapshots()[0].raw(), "20230101");
        assert_eq!(home.volume.snapshots()[1].raw(), "20230102");

        let server = &result["my.server.root"];
        assert_eq!(server.volume.orig_name(), "my.server.root");
        assert_eq!(server.volume.snapshots().len(), 1);
    }

    #[test]
    fn test_long_timestamp_format() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data.20230101T1531")).unwrap();
        std::fs::create_dir(dir.path().join("data.20230102T1015_1")).unwrap();

        let spec = make_spec(dir.path());
        let result = inspect_source(&spec).unwrap();

        let data = &result["data"];
        assert_eq!(data.volume.snapshots().len(), 2);
        assert_eq!(data.volume.snapshots()[0].timestamp(), "20230101T1531");
        assert_eq!(data.volume.snapshots()[0].seq(), None);
        assert_eq!(data.volume.snapshots()[1].timestamp(), "20230102T1015");
        assert_eq!(data.volume.snapshots()[1].seq(), Some("_1"));
    }
}

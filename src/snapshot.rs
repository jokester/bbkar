use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub _snapshots_dir: PathBuf,
    pub volume: String,
    pub timestamp: String,
    pub gen: Option<u32>,
}

impl Snapshot {
    fn match_basename(basename: &str) -> Option<(String, String, Option<u32>)> {
        /**
        group 1: basename of orig subvolume
        group 2: timestamp: short / long / long-iso
        group 3:
         */
        let subvol_pattern: Regex =
            Regex::new(r"^(?<volume>.*)\.(?<timestamp>[T\d+]{8,})(?<gen>_\d+)?$").unwrap();
        let matched = subvol_pattern.captures(basename)?;
        let volume = matched.name("volume")?.as_str().to_string();
        let timestamp = matched.name("timestamp")?.as_str().to_string();
        let gen = matched.name("gen").map(|m| {
            m.as_str()
                .strip_prefix('_')
                .unwrap()
                .parse::<u32>()
                .unwrap()
        });
        Some((volume, timestamp, gen))
    }
    pub fn from_pathbuf(buffer: &Path) -> Option<Snapshot> {
        let _snapshots_dir = buffer.parent()?;
        let last_component = buffer.components().last()?.as_os_str().to_str()?;
        let (volume, timestamp, gen) = Self::match_basename(last_component)?;
        Some(Snapshot {
            _snapshots_dir: _snapshots_dir.to_path_buf(),
            volume: volume.to_string(),
            timestamp: timestamp.to_string(),
            gen,
        })
    }
}

pub struct SnapshotTimestamp {
    timestamp: String,
    step: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognize_valid_basename() {
        // assert_eq!(valid_path.file_name(), None);
        let m = Snapshot::match_basename("c.20230101");
        assert_eq!(m, Some(("c".to_string(), "20230101".to_string(), None)));

        let m = Snapshot::match_basename("c.20230101T2+3_3");
        assert_eq!(
            m,
            Some(("c".to_string(), "20230101T2+3".to_string(), Some(3)))
        )
    }
    #[test]
    fn recognize_invalid_basename() {
        let m = Snapshot::match_basename("c.2023001");
        assert_eq!(m, None);

        let m = Snapshot::match_basename("c");
        assert_eq!(m, None)
    }
}

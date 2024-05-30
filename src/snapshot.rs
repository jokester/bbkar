use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Snapshot {
    pub _snapshots_dir: PathBuf,
    pub volume: String,
    pub timestamp: String,
    pub gen: Option<u32>,
}

impl Snapshot {
    pub fn from_pathbuf(buffer: &Path) -> Option<Snapshot> {
        /**
        group 1: basename of orig subvolume
        group 2: timestamp: short / long / long-iso
        group 3:

        */
        let subvol_pattern: Regex =
            Regex::new(r"^(<volume>.*).(<timestamp>[T\d+]{8,})(<gen>_\d+)?$").unwrap();
        let _snapshots_dir = buffer.parent()?;
        let last_component = buffer.components().last()?;
        let last_component = last_component.as_os_str().to_str()?;
        let matched = subvol_pattern.captures(last_component)?;
        let volume = matched.name("volume")?.as_str();
        let timestamp = &matched.name("timestamp")?.as_str();
        let gen = matched
            .name("gen")
            .map(|m| m.as_str().parse::<u32>().unwrap())
            .clone();

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
    fn extract_path() {}
}

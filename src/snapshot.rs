use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    pub _snapshots_dir: &'a Path,
    pub volume: &'a str,
    pub timestamp: &'a str,
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
        let _snapshots_dir= buffer.parent()?.clone();
        let last_component = buffer.components().last()?;
        let last_component = last_component.as_os_str().to_str()?;
        let matched = subvol_pattern.captures(last_component)?;
        let volume = matched.name("volume")?.as_str();
        let timestamp = &matched.name("timestamp")?.as_str();
        let gen = &matched
            .name("gen")
            .map(|m| m.as_str().parse::<u32>().unwrap());

        Some(Snapshot {
            _snapshots_dir,
            volume: volume.clone(),
            timestamp: timestamp.clone(),
            gen: gen.clone(),
        })
    }
    pub fn from_string() -> Option<Snapshot> {
        todo!()
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

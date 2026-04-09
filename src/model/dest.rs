use crate::Timestamp;

/**
 * The state of a `LOCATION/VOLUME/` backup location
 */
#[derive(serde::Deserialize, serde::Serialize, Debug, PartialEq, Eq, Clone)]
pub struct DestState {
    pub meta: Option<DestMeta>,
}

/**
 * The content of a `LOCATION/VOLUME/bbkar-meta.yaml`
 * NOTE Updating this part in the dest storage should be carefully made, to make the backup operations atomic and safe.
 */
#[derive(serde::Serialize, Debug, PartialEq, Eq, Clone)]
pub struct DestMeta {
    pub first_sync_timestamp: u64,
    pub last_sync_timestamp: u64,
    // sorted by timestamp
    archives: Vec<VolumeArchive>,
}

impl DestMeta {
    pub fn new(
        first_sync_timestamp: u64,
        last_sync_timestamp: u64,
        mut archives: Vec<VolumeArchive>,
    ) -> Self {
        archives.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Self {
            first_sync_timestamp,
            last_sync_timestamp,
            archives,
        }
    }

    pub fn archives(&self) -> &[VolumeArchive] {
        &self.archives
    }

    pub fn oldest_archive(&self) -> Option<&VolumeArchive> {
        self.archives.first()
    }
    pub fn newest_archive(&self) -> Option<&VolumeArchive> {
        self.archives.last()
    }
    pub fn total_size(&self) -> u64 {
        self.archives.iter().map(|a| a.total_size()).sum()
    }
    pub fn add_archive(&mut self, archive: VolumeArchive) {
        self.archives.push(archive);
        self.archives.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    pub fn set_last_sync_timestamp(&mut self, ts: u64) {
        self.last_sync_timestamp = ts;
    }

    pub fn total_size_readable(&self) -> String {
        let total = self.total_size();
        if total >= 1024 * 1024 * 1024 {
            format!("{:.1} GiB", total as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if total >= 1024 * 1024 {
            format!("{:.1} MiB", total as f64 / (1024.0 * 1024.0))
        } else if total >= 1024 {
            format!("{:.1} KiB", total as f64 / 1024.0)
        } else {
            format!("{} bytes", total)
        }
    }
}

// Custom Deserialize to ensure archives are sorted after loading
impl<'de> serde::Deserialize<'de> for DestMeta {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            first_sync_timestamp: u64,
            last_sync_timestamp: u64,
            archives: Vec<VolumeArchive>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(DestMeta::new(
            raw.first_sync_timestamp,
            raw.last_sync_timestamp,
            raw.archives,
        ))
    }
}

/**
 * Copy of a source snapshot volume, identified by its timestamp suffix.
 */
#[derive(serde::Deserialize, serde::Serialize, Debug, PartialEq, Eq, Clone)]
pub struct VolumeArchive {
    /// The btrbk timestamp suffix (e.g. "20250101", "20250101T1531_1")
    #[serde(rename = "snapshot")]
    pub timestamp: Timestamp,
    /// Parent timestamp for incremental backups, None for full backups
    #[serde(rename = "parent_snapshot")]
    pub parent_timestamp: Option<String>,
    pub chunks: Vec<ChunkFilename>,
}
impl VolumeArchive {
    pub fn is_incremental(&self) -> bool {
        self.parent_timestamp.is_some()
    }

    pub fn total_size(&self) -> u64 {
        self.chunks.iter().map(|c| c.size as u64).sum()
    }

    pub fn total_raw_size(&self) -> Option<u64> {
        self.chunks
            .iter()
            .map(|c| c.raw_size)
            .try_fold(0u64, |acc, size| size.map(|size| acc + size))
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, PartialEq, Eq, Clone)]
pub struct ChunkFilename {
    filename: String,
    size: u32,
    sha256sum: Option<String>,
    compression: Option<String>,
    /// Size before compression and encryption
    raw_size: Option<u64>,
}

impl ChunkFilename {
    pub fn new(
        filename: String,
        size: u32,
        compression: Option<String>,
        raw_size: Option<u64>,
        sha256sum: Option<String>,
    ) -> Self {
        Self {
            filename,
            size,
            sha256sum,
            compression,
            raw_size,
        }
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn raw_size(&self) -> Option<u64> {
        self.raw_size
    }
}

/**
 * A list of subvolumes, created by snapshotting a source volume
 */
#[derive(Clone, Debug)]
pub struct Series {
    orig_name: String,
    // sorted by timestamp, newest last. always non-empty.
    snapshots: Vec<Timestamp>, // TIMESTAMP or TIMESTAMP_ID for btrfs
}

impl Series {
    pub fn new(orig_name: String, mut snapshots: Vec<Timestamp>) -> Self {
        snapshots.sort();
        Self {
            orig_name,
            snapshots,
        }
    }

    pub fn orig_name(&self) -> &str {
        &self.orig_name
    }

    pub fn snapshots(&self) -> &[Timestamp] {
        &self.snapshots
    }

    pub fn oldest_snapshot(&self) -> &Timestamp {
        self.snapshots.first().expect("snapshots is non-empty")
    }
    pub fn newest_snapshot(&self) -> &Timestamp {
        self.snapshots.last().expect("snapshots is non-empty")
    }
}

/**
 * the suffix created by btrbk
 * - short YYYYMMDD[_N] (e.g. "20150825", "20150825_1")
 * - long YYYYMMDD<T>hhmm[_N] (e.g. "20150825T1531") (the default)
 * - long-iso YYYYMMDD<T>hhmmss±hhmm[_N] (e.g. "20150825T153123+0200")
 */
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub raw: String,
    timestamp: String,
    seq: Option<String>, // e.g. "_1"
}

impl Timestamp {
    /// Parse a snapshot suffix string (the part after `VOLUME.`) into a Timestamp.
    ///
    /// The suffix is `TIMESTAMP[_N]` where the optional `_N` is a sequence number.
    /// Timestamps may contain `T`, `+`, `-` and digits but never `_`,
    /// so we split on the last `_` only if the part after it is purely numeric.
    pub fn parse(suffix: &str) -> Option<Self> {
        if suffix.is_empty() {
            return None;
        }
        let (timestamp, seq) = match suffix.rfind('_') {
            Some(pos) => {
                let after = &suffix[pos + 1..];
                if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                    (suffix[..pos].to_string(), Some(format!("_{}", after)))
                } else {
                    (suffix.to_string(), None)
                }
            }
            None => (suffix.to_string(), None),
        };
        Some(Self {
            raw: suffix.to_string(),
            timestamp,
            seq,
        })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn seq(&self) -> Option<&str> {
        self.seq.as_deref()
    }
}

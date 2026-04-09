use crate::model::config::DestSpec;
use crate::model::dest::{DestMeta, DestState};
use crate::model::error::BR;
use tracing::debug;

use super::opendal::{path_in_volume, summon_blocking_operator};

const META_FILENAME: &str = "bbkar-meta.yaml";

pub fn inspect_dest_volume(spec: &DestSpec, volume: &str) -> BR<DestState> {
    let op = summon_blocking_operator(spec)?;
    let meta_path = path_in_volume(volume, META_FILENAME);
    debug!(volume = %volume, dest = %spec.display_location(), meta_path = %meta_path, "reading destination metadata");

    match op.read(&meta_path) {
        Ok(data) => {
            let content = String::from_utf8(data.to_vec())
                .map_err(|e| opendal::Error::new(opendal::ErrorKind::Unexpected, e.to_string()))?;
            let meta: DestMeta = serde_yaml::from_str(&content)?;
            debug!(
                volume = %volume,
                archives = meta.archives().len(),
                last_sync_timestamp = meta.last_sync_timestamp,
                "destination metadata loaded"
            );
            Ok(DestState { meta: Some(meta) })
        }
        Err(e) if e.kind() == opendal::ErrorKind::NotFound => {
            debug!(volume = %volume, "destination metadata not found");
            Ok(DestState { meta: None })
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod test_inspect_dest {
    use super::*;

    #[test]
    fn test_missing_meta_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let spec = DestSpec {
            backend_spec: crate::model::config::BackendSpec::Local {
                path: dir.path().to_string_lossy().to_string(),
            },
        };
        let state = inspect_dest_volume(&spec, "nonexistent").unwrap();
        assert_eq!(state.meta, None);
    }

    #[test]
    fn test_reads_existing_meta() {
        let dir = tempfile::tempdir().unwrap();
        let vol_dir = dir.path().join("myvolume");
        std::fs::create_dir_all(&vol_dir).unwrap();
        std::fs::write(
            vol_dir.join(META_FILENAME),
            "first_sync_timestamp: 1000\nlast_sync_timestamp: 2000\narchives: []\n",
        )
        .unwrap();

        let spec = DestSpec {
            backend_spec: crate::model::config::BackendSpec::Local {
                path: dir.path().to_string_lossy().to_string(),
            },
        };
        let state = inspect_dest_volume(&spec, "myvolume").unwrap();
        let meta = state.meta.unwrap();
        assert_eq!(meta.first_sync_timestamp, 1000);
        assert_eq!(meta.last_sync_timestamp, 2000);
        assert!(meta.archives().is_empty());
    }
}

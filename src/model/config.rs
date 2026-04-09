use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::BR;
use crate::utils::duration::{CalendarDuration, PreserveSchedule, Weekday};

/// Configuration file structure for bbkar
#[derive(Debug, Deserialize, Serialize)]
pub struct BbkarConfigFile {
    #[serde(default)]
    pub global: GlobalConfig,
    /// souce.name => source
    pub source: HashMap<String, SourceSpec>,
    /// dest.name => dest
    pub dest: HashMap<String, DestSpec>,
    // sync.name => (source.name, dest.name, ...)
    pub sync: HashMap<String, SyncSpec>,
}

impl BbkarConfigFile {
    pub fn from_toml(content: &str) -> BR<Self> {
        use super::error::BbkarError;

        let config: Self = toml::from_str(content)?;
        let mut errors = Vec::new();

        // global: field value ranges
        if config.global.compression != "zstd" {
            errors.push(format!(
                "global.compression must be \"zstd\", got \"{}\"",
                config.global.compression
            ));
        }
        if config.global.max_backup_chunk_size < 1 || config.global.max_backup_chunk_size > 2048 {
            errors.push(format!(
                "global.max_backup_chunk_size must be in range 1..=2048, got {}",
                config.global.max_backup_chunk_size
            ));
        }
        if config.global.btrfs_send_concurrency != 1 {
            errors.push(format!(
                "global.btrfs_send_concurrency must be 1, got {}",
                config.global.btrfs_send_concurrency
            ));
        }
        if config.global.write_archive_concurrency != 1 {
            errors.push(format!(
                "global.write_archive_concurrency must be 1, got {}",
                config.global.write_archive_concurrency
            ));
        }

        // at most 1 source, dest, sync
        if config.source.len() > 1 {
            errors.push(format!(
                "at most 1 source is supported, got {}",
                config.source.len()
            ));
        }
        if config.dest.len() > 1 {
            errors.push(format!(
                "at most 1 dest is supported, got {}",
                config.dest.len()
            ));
        }
        if config.sync.len() > 1 {
            errors.push(format!(
                "at most 1 sync is supported, got {}",
                config.sync.len()
            ));
        }

        // dest: backend-specific validation
        for (name, dest) in &config.dest {
            match &dest.backend_spec {
                BackendSpec::Local { path } => {
                    if !path.starts_with('/') {
                        errors.push(format!(
                            "dest.{}.path must be absolute for local backend, got \"{}\"",
                            name, path
                        ));
                    }
                }
                BackendSpec::S3 { bucket, .. } | BackendSpec::Gcs { bucket, .. } => {
                    if bucket.trim().is_empty() {
                        errors.push(format!("dest.{}.bucket must not be empty", name));
                    }
                }
            }
        }

        // sync: must refer to existing source and dest
        for (name, sync) in &config.sync {
            if !config.source.contains_key(&sync.source) {
                errors.push(format!(
                    "sync.{}.source refers to unknown source \"{}\"",
                    name, sync.source
                ));
            }
            if !config.dest.contains_key(&sync.dest) {
                errors.push(format!(
                    "sync.{}.dest refers to unknown dest \"{}\"",
                    name, sync.dest
                ));
            }
            // validate sending policy fields
            if CalendarDuration::parse(&sync.min_full_send_interval).is_none() {
                errors.push(format!(
                    "sync.{}.min_full_send_interval: invalid duration \"{}\"",
                    name, sync.min_full_send_interval
                ));
            }
            if let Some(depth) = sync.max_incremental_depth
                && depth < 1
            {
                errors.push(format!(
                    "sync.{}.max_incremental_depth must be >= 1, got {}",
                    name, depth
                ));
            }
            // validate retention policy fields
            if sync.archive_preserve_min != "all"
                && CalendarDuration::parse(&sync.archive_preserve_min).is_none()
            {
                errors.push(format!(
                    "sync.{}.archive_preserve_min: must be \"all\" or valid duration, got \"{}\"",
                    name, sync.archive_preserve_min
                ));
            }
            if let Some(ref schedule) = sync.archive_preserve
                && PreserveSchedule::parse(schedule).is_none()
            {
                errors.push(format!(
                    "sync.{}.archive_preserve: invalid schedule \"{}\"",
                    name, schedule
                ));
            }
            if Weekday::parse(&sync.preserve_day_of_week).is_none() {
                errors.push(format!(
                    "sync.{}.preserve_day_of_week: invalid weekday \"{}\"",
                    name, sync.preserve_day_of_week
                ));
            }
        }

        if !errors.is_empty() {
            return Err(BbkarError::Config(errors));
        }

        // Normalize paths: strip trailing slashes to avoid "//" in constructed paths
        let mut config = config;
        for source in config.source.values_mut() {
            while source.path.len() > 1 && source.path.ends_with('/') {
                source.path.pop();
            }
        }

        Ok(config)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(default = "default_compression")] // must be "zstd"
    pub compression: String,
    #[serde(default = "default_max_backup_chunk_size")] // range: 1 ~ 2048
    pub max_backup_chunk_size: u64,
    #[serde(default = "default_one")] // range: 1~1
    pub btrfs_send_concurrency: u32,
    #[serde(default = "default_one")] // range: 1~1
    pub write_archive_concurrency: u32,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            compression: default_compression(),
            max_backup_chunk_size: default_max_backup_chunk_size(),
            btrfs_send_concurrency: default_one(),
            write_archive_concurrency: default_one(),
        }
    }
}

fn default_compression() -> String {
    "zstd".to_string()
}

fn default_max_backup_chunk_size() -> u64 {
    32
}

fn default_one() -> u32 {
    1
}

fn default_wildcard_vec() -> Vec<String> {
    vec!["*".to_string()]
}

/// Source specification for btrfs subvolumes
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SourceSpec {
    pub path: String,
    #[serde(default = "default_wildcard_vec")]
    pub filter: Vec<String>,
}

impl SourceSpec {
    pub fn build_path(&self, rest: &str) -> String {
        format!("{}/{}", self.path, rest)
    }
}

/// Destination specification for archive storage
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DestSpec {
    #[serde(flatten)]
    pub backend_spec: BackendSpec,
}

impl DestSpec {
    pub fn backend_spec(&self) -> &BackendSpec {
        &self.backend_spec
    }

    pub fn root_path(&self) -> &str {
        match &self.backend_spec {
            BackendSpec::Local { path } => path,
            BackendSpec::S3 { path, .. } => path,
            BackendSpec::Gcs { path, .. } => path,
        }
    }

    pub fn display_location(&self) -> String {
        match &self.backend_spec {
            BackendSpec::Local { path } => path.trim_end_matches('/').to_string(),
            BackendSpec::S3 { bucket, path, .. } => {
                if path.is_empty() {
                    format!("s3://{}", bucket)
                } else {
                    format!("s3://{}/{}", bucket, path.trim_matches('/'))
                }
            }
            BackendSpec::Gcs { bucket, path, .. } => {
                if path.is_empty() {
                    format!("gcs://{}", bucket)
                } else {
                    format!("gcs://{}/{}", bucket, path.trim_matches('/'))
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "driver")]
pub enum BackendSpec {
    #[serde(rename = "local")]
    Local { path: String },
    #[serde(rename = "s3")]
    S3 {
        bucket: String,
        path: String,
        region: Option<String>,
        endpoint: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        session_token: Option<String>,
        #[serde(default)]
        disable_config_load: bool,
    },
    #[serde(rename = "gcs")]
    Gcs {
        bucket: String,
        path: String,
        endpoint: Option<String>,
        credential_path: Option<String>,
    },
}

// sync specification for synchronizing sources to destinations
#[derive(Debug, Deserialize, Serialize)]
pub struct SyncSpec {
    pub source: String,
    pub dest: String,
    #[serde(default = "default_wildcard_vec")]
    pub filter: Vec<String>,
    // Sending policy
    #[serde(default = "default_min_full_send_interval")]
    pub min_full_send_interval: String,
    pub max_incremental_depth: Option<u32>,
    // Retention policy
    #[serde(default = "default_preserve_min")]
    pub archive_preserve_min: String,
    pub archive_preserve: Option<String>,
    #[serde(default = "default_preserve_day_of_week")]
    pub preserve_day_of_week: String,
}

fn default_min_full_send_interval() -> String {
    "1w".to_string()
}

fn default_preserve_min() -> String {
    "all".to_string()
}

fn default_preserve_day_of_week() -> String {
    "sunday".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::BbkarError;

    fn base_config_with_dest(dest_block: &str) -> String {
        format!(
            r#"[global]

[source.src1]
path = "/snapshots"

{dest_block}

[sync.main]
source = "src1"
dest = "dst1"
filter = ["*"]
"#,
            dest_block = dest_block
        )
    }

    #[test]
    fn test_parse_local_dest_backend() {
        let config = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "local"
path = "/backup/local"
"#,
        ))
        .unwrap();

        let dest = config.dest.get("dst1").unwrap();
        match dest.backend_spec() {
            BackendSpec::Local { path } => assert_eq!(path, "/backup/local"),
            other => panic!("expected local backend, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_s3_dest_backend() {
        let config = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "s3"
bucket = "archive-bucket"
path = "bbkar"
region = "us-east-1"
endpoint = "http://127.0.0.1:9000"
access_key_id = "ak"
secret_access_key = "sk"
"#,
        ))
        .unwrap();

        let dest = config.dest.get("dst1").unwrap();
        match dest.backend_spec() {
            BackendSpec::S3 {
                bucket,
                path,
                region,
                endpoint,
                access_key_id,
                secret_access_key,
                session_token,
                disable_config_load,
            } => {
                assert_eq!(bucket, "archive-bucket");
                assert_eq!(path, "bbkar");
                assert_eq!(region.as_deref(), Some("us-east-1"));
                assert_eq!(endpoint.as_deref(), Some("http://127.0.0.1:9000"));
                assert_eq!(access_key_id.as_deref(), Some("ak"));
                assert_eq!(secret_access_key.as_deref(), Some("sk"));
                assert_eq!(session_token, &None);
                assert!(!disable_config_load);
            }
            other => panic!("expected s3 backend, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_s3_dest_backend_disable_config_load() {
        let config = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "s3"
bucket = "archive-bucket"
path = "bbkar"
disable_config_load = true
"#,
        ))
        .unwrap();

        let dest = config.dest.get("dst1").unwrap();
        match dest.backend_spec() {
            BackendSpec::S3 {
                disable_config_load,
                ..
            } => {
                assert!(*disable_config_load);
            }
            other => panic!("expected s3 backend, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_gcs_dest_backend() {
        let config = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "gcs"
bucket = "archive-bucket"
path = "bbkar"
endpoint = "http://127.0.0.1:4443"
credential_path = "/tmp/gcs.json"
"#,
        ))
        .unwrap();

        let dest = config.dest.get("dst1").unwrap();
        match dest.backend_spec() {
            BackendSpec::Gcs {
                bucket,
                path,
                endpoint,
                credential_path,
            } => {
                assert_eq!(bucket, "archive-bucket");
                assert_eq!(path, "bbkar");
                assert_eq!(endpoint.as_deref(), Some("http://127.0.0.1:4443"));
                assert_eq!(credential_path.as_deref(), Some("/tmp/gcs.json"));
            }
            other => panic!("expected gcs backend, got {:?}", other),
        }
    }

    #[test]
    fn test_local_dest_requires_absolute_path() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "local"
path = "relative/path"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Config(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.contains("dest.dst1.path must be absolute"))
                )
            }
            other => panic!("expected config error, got {:?}", other),
        }
    }

    #[test]
    fn test_s3_dest_requires_non_empty_bucket() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "s3"
bucket = "   "
path = "bbkar"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Config(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.contains("dest.dst1.bucket must not be empty"))
                )
            }
            other => panic!("expected config error, got {:?}", other),
        }
    }

    #[test]
    fn test_dest_driver_is_whitelisted() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "r2"
bucket = "archive-bucket"
path = "bbkar"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Toml(err) => {
                let msg = err.to_string();
                assert!(msg.contains("unknown variant"));
                assert!(msg.contains("local"));
                assert!(msg.contains("s3"));
                assert!(msg.contains("gcs"));
            }
            other => panic!("expected toml error, got {:?}", other),
        }
    }

    #[test]
    fn test_s3_dest_requires_bucket_field() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "s3"
path = "bbkar"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Toml(err) => {
                assert!(err.to_string().contains("missing field `bucket`"));
            }
            other => panic!("expected toml error, got {:?}", other),
        }
    }

    #[test]
    fn test_gcs_dest_requires_path_field() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "gcs"
bucket = "archive-bucket"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Toml(err) => {
                assert!(err.to_string().contains("missing field `path`"));
            }
            other => panic!("expected toml error, got {:?}", other),
        }
    }

    #[test]
    fn test_gcs_dest_requires_bucket_field() {
        let err = BbkarConfigFile::from_toml(&base_config_with_dest(
            r#"[dest.dst1]
driver = "gcs"
path = "bbkar"
"#,
        ))
        .unwrap_err();

        match err {
            BbkarError::Toml(err) => {
                assert!(err.to_string().contains("missing field `bucket`"));
            }
            other => panic!("expected toml error, got {:?}", other),
        }
    }
}

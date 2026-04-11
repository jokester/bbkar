use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;

use opendal::Operator;
use opendal::blocking::Operator as BlockingOperator;
use tokio::runtime::Runtime;
use tracing::{debug, warn};

use crate::model::config::{BackendSpec, DestSpec};
use crate::model::error::BR;

static OPENDAL_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("failed to initialize OpenDAL runtime"));

pub fn summon_operator(dest_spec: &DestSpec) -> BR<Operator> {
    debug!(backend = %dest_spec.display_location(), "creating OpenDAL operator");
    let operator = match dest_spec.backend_spec() {
        BackendSpec::Local { path } => {
            debug!(root = %path, "configuring local operator");
            let builder = opendal::services::Fs::default().root(path);
            Operator::new(builder)?.finish()
        }
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
            debug!(
                bucket = %bucket,
                root = %path,
                region = ?region,
                endpoint = ?endpoint,
                access_key_id = %access_key_id.as_ref().map(|_| "<set>").unwrap_or("<none>"),
                secret_access_key = %secret_access_key.as_ref().map(|_| "<set>").unwrap_or("<none>"),
                session_token = %session_token.as_ref().map(|_| "<set>").unwrap_or("<none>"),
                disable_config_load = *disable_config_load,
                "configuring S3 operator"
            );
            let mut builder = opendal::services::S3::default().bucket(bucket).root(path);
            if let Some(region) = region {
                builder = builder.region(region);
            }
            if let Some(endpoint) = endpoint {
                builder = builder.endpoint(endpoint);
            }
            if let Some(access_key_id) = access_key_id {
                builder = builder.access_key_id(access_key_id);
            }
            if let Some(secret_access_key) = secret_access_key {
                builder = builder.secret_access_key(secret_access_key);
            }
            if let Some(session_token) = session_token {
                builder = builder.session_token(session_token);
            }
            if *disable_config_load {
                builder = builder.disable_config_load();
            } else if access_key_id.is_none()
                && secret_access_key.is_none()
                && session_token.is_none()
            {
                warn!(
                    "S3 credentials are not set in config; OpenDAL will load AWS credentials from default locations like environment variables, ~/.aws/credentials, and ~/.aws/config"
                );
            }
            Operator::new(builder)?.finish()
        }
        BackendSpec::Gcs {
            bucket,
            path,
            endpoint,
            credential_path,
        } => {
            debug!(
                bucket = %bucket,
                root = %path,
                endpoint = ?endpoint,
                credential_path = ?credential_path,
                "configuring GCS operator"
            );
            let mut builder = opendal::services::Gcs::default().bucket(bucket).root(path);
            if let Some(endpoint) = endpoint {
                builder = builder.endpoint(endpoint);
            }
            if let Some(credential_path) = credential_path {
                debug!(credential_path = %credential_path, "setting GCS credential path");
                builder = builder.credential_path(credential_path);
            } else if default_gcloud_adc_path_exists() {
                warn!(
                    "GCS credential_path is not set and ~/.config/gcloud/application_default_credentials.json is not supported yet. You can safely ignore this when running inside GCP."
                );
            }
            debug!("building GCS operator");
            let op = Operator::new(builder)?.finish();
            debug!("GCS operator created");
            op
        }
    };

    Ok(operator)
}

pub fn summon_blocking_operator(dest_spec: &DestSpec) -> BR<BlockingOperator> {
    debug!("creating blocking OpenDAL operator");
    let operator = summon_operator(dest_spec)?;
    let _guard = OPENDAL_RUNTIME.enter();
    debug!("wrapping operator for blocking access");
    let bop = BlockingOperator::new(operator)?;
    debug!("blocking OpenDAL operator ready");
    Ok(bop)
}

pub fn path_in_volume(volume: &str, name: &str) -> String {
    format!("{}/{}", volume.trim_matches('/'), name.trim_matches('/'))
}

pub fn path_in_snapshot(volume: &str, snapshot: &str, name: &str) -> String {
    format!(
        "{}/{}/{}",
        volume.trim_matches('/'),
        snapshot.trim_matches('/'),
        name.trim_matches('/')
    )
}

fn default_gcloud_adc_path_exists() -> bool {
    let Some(home) = env::var_os("HOME") else {
        return false;
    };
    let path = PathBuf::from(home)
        .join(".config")
        .join("gcloud")
        .join("application_default_credentials.json");
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn local_dest(path: &std::path::Path) -> DestSpec {
        DestSpec {
            backend_spec: BackendSpec::Local {
                path: path.to_string_lossy().to_string(),
            },
        }
    }

    fn s3_dest(disable_config_load: bool) -> DestSpec {
        DestSpec {
            backend_spec: BackendSpec::S3 {
                bucket: "bucket".to_string(),
                path: "root".to_string(),
                region: Some("ap-northeast-1".to_string()),
                endpoint: Some("http://127.0.0.1:9000".to_string()),
                access_key_id: Some("key".to_string()),
                secret_access_key: Some("secret".to_string()),
                session_token: None,
                disable_config_load,
            },
        }
    }

    fn gcs_dest(with_credential_path: bool) -> DestSpec {
        DestSpec {
            backend_spec: BackendSpec::Gcs {
                bucket: "bucket".to_string(),
                path: "root".to_string(),
                endpoint: Some("http://127.0.0.1:4443".to_string()),
                credential_path: with_credential_path.then(|| "/tmp/fake-creds.json".to_string()),
            },
        }
    }

    #[test]
    fn test_path_helpers_trim_slashes() {
        assert_eq!(path_in_volume("/vol/", "/meta.yaml/"), "vol/meta.yaml");
        assert_eq!(
            path_in_snapshot("/vol/", "/snap/", "/part.zst/"),
            "vol/snap/part.zst"
        );
    }

    #[test]
    fn test_default_gcloud_adc_path_exists_without_home() {
        let _guard = env_lock().lock().unwrap();
        let old_home = env::var_os("HOME");
        unsafe {
            env::remove_var("HOME");
        }

        assert!(!default_gcloud_adc_path_exists());

        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            }
        }
    }

    #[test]
    fn test_default_gcloud_adc_path_exists_with_adc_file() {
        let _guard = env_lock().lock().unwrap();
        let old_home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        let adc_path = tmp
            .path()
            .join(".config")
            .join("gcloud")
            .join("application_default_credentials.json");
        std::fs::create_dir_all(adc_path.parent().unwrap()).unwrap();
        std::fs::write(&adc_path, "{}").unwrap();
        unsafe {
            env::set_var("HOME", tmp.path());
        }

        assert!(default_gcloud_adc_path_exists());

        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn test_default_gcloud_adc_path_exists_without_adc_file() {
        let _guard = env_lock().lock().unwrap();
        let old_home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            env::set_var("HOME", tmp.path());
        }

        assert!(!default_gcloud_adc_path_exists());

        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn test_summon_operator_local_backend_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = local_dest(tmp.path());
        let op = summon_operator(&spec).unwrap();

        let data = OPENDAL_RUNTIME.block_on(async {
            op.write("vol/meta.yaml", "hello".as_bytes().to_vec())
                .await?;
            op.read("vol/meta.yaml").await
        });

        assert_eq!(String::from_utf8(data.unwrap().to_vec()).unwrap(), "hello");
    }

    #[test]
    fn test_summon_blocking_operator_local_backend_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = local_dest(tmp.path());
        let op = summon_blocking_operator(&spec).unwrap();

        op.write("vol/meta.yaml", b"world".to_vec()).unwrap();
        let data = op.read("vol/meta.yaml").unwrap();

        assert_eq!(String::from_utf8(data.to_vec()).unwrap(), "world");
    }

    #[test]
    fn test_summon_operator_builds_s3_operator_without_network_access() {
        let op = summon_operator(&s3_dest(true)).unwrap();
        let info = op.info();

        assert_eq!(info.scheme(), "s3");
    }

    #[test]
    fn test_summon_operator_builds_gcs_operator_without_network_access() {
        let op = summon_operator(&gcs_dest(true)).unwrap();
        let info = op.info();

        assert_eq!(info.scheme(), "gcs");
    }

    #[test]
    fn test_summon_operator_builds_gcs_operator_without_credential_path() {
        let _guard = env_lock().lock().unwrap();
        let old_home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            env::set_var("HOME", tmp.path());
        }

        let op = summon_operator(&gcs_dest(false)).unwrap();
        let info = op.info();
        assert_eq!(info.scheme(), "gcs");

        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
    }
}

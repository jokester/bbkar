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

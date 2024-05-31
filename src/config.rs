use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Result as IR;
use std::path::PathBuf;

pub struct ConfigReader {
    path: str,
}

pub fn parse_config(config: &str) -> () {
    let f: HashMap<String, Profile> = toml::from_str(config).unwrap();
    todo!();
}

impl ConfigReader {
    fn read(&self) -> IR<Profile> {
        todo!()
    }
}

pub struct ConfigFile {}
#[derive(Serialize, Deserialize)]
pub struct Profile {
    snapshot_dir: PathBuf,
    _volumes: Option<Vec<String>>,
    _archive_prefix: Option<String>,
}

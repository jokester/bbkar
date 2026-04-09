use std::io;
use toml::de;

pub type BR<T> = Result<T, BbkarError>;

#[derive(Debug)]
pub enum BbkarError {
    Io(io::Error),
    Toml(de::Error),
    Yaml(serde_yaml::Error),
    OpenDal(opendal::Error),
    Config(Vec<String>),
    Plan(String),
    Execution(String),
    InvalidSourcePath(String),
    OtherError(String),
}

impl std::error::Error for BbkarError {}

impl std::fmt::Display for BbkarError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BbkarError::Io(err) => write!(f, "IO Error: {}", err),
            BbkarError::Toml(err) => write!(f, "TOML Error: {}", err),
            BbkarError::Yaml(err) => write!(f, "YAML Error: {}", err),
            BbkarError::OpenDal(err) => write!(f, "OpenDAL Error: {}", err),
            BbkarError::Config(msg) => write!(f, "Configuration Error: {:#?}", msg),
            BbkarError::Plan(msg) => write!(f, "Plan Error: {}", msg),
            BbkarError::Execution(msg) => write!(f, "Execution Error: {}", msg),
            BbkarError::InvalidSourcePath(path) => {
                write!(f, "Invalid source path: {}", path)
            }
            BbkarError::OtherError(msg) => write!(f, "Other Error: {}", msg),
        }
    }
}

impl From<io::Error> for BbkarError {
    fn from(value: io::Error) -> Self {
        BbkarError::Io(value)
    }
}

impl From<de::Error> for BbkarError {
    fn from(value: de::Error) -> Self {
        BbkarError::Toml(value)
    }
}

impl From<serde_yaml::Error> for BbkarError {
    fn from(value: serde_yaml::Error) -> Self {
        BbkarError::Yaml(value)
    }
}

impl From<opendal::Error> for BbkarError {
    fn from(value: opendal::Error) -> Self {
        BbkarError::OpenDal(value)
    }
}

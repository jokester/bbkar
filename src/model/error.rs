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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_variants() {
        assert_eq!(
            BbkarError::Config(vec!["bad config".to_string()]).to_string(),
            "Configuration Error: [\n    \"bad config\",\n]"
        );
        assert_eq!(BbkarError::Plan("bad plan".to_string()).to_string(), "Plan Error: bad plan");
        assert_eq!(
            BbkarError::Execution("bad exec".to_string()).to_string(),
            "Execution Error: bad exec"
        );
        assert_eq!(
            BbkarError::InvalidSourcePath("/bad".to_string()).to_string(),
            "Invalid source path: /bad"
        );
        assert_eq!(
            BbkarError::OtherError("misc".to_string()).to_string(),
            "Other Error: misc"
        );
    }

    #[test]
    fn test_from_io_error() {
        let err: BbkarError = io::Error::other("boom").into();
        assert!(matches!(err, BbkarError::Io(_)));
        assert!(err.to_string().contains("IO Error: boom"));
    }

    #[test]
    fn test_from_toml_error() {
        let err = toml::from_str::<toml::Value>("not = [valid").unwrap_err();
        let err: BbkarError = err.into();
        assert!(matches!(err, BbkarError::Toml(_)));
        assert!(err.to_string().contains("TOML Error:"));
    }

    #[test]
    fn test_from_yaml_error() {
        let err = serde_yaml::from_str::<serde_yaml::Value>("key: [").unwrap_err();
        let err: BbkarError = err.into();
        assert!(matches!(err, BbkarError::Yaml(_)));
        assert!(err.to_string().contains("YAML Error:"));
    }
}

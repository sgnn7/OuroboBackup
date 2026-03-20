use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OuroboError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("config file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("watch error on {path}: {message}")]
    Watch { path: PathBuf, message: String },

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("TOML parse error: {0}")]
    TomlParse(String),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(String),

    #[error("duplicate watch ID: {0}")]
    DuplicateWatch(String),

    #[error("watch not found: {0}")]
    WatchNotFound(String),
}

pub type Result<T> = std::result::Result<T, OuroboError>;

impl From<serde_json::Error> for OuroboError {
    fn from(e: serde_json::Error) -> Self {
        OuroboError::Serialization(e.to_string())
    }
}

impl From<toml::de::Error> for OuroboError {
    fn from(e: toml::de::Error) -> Self {
        OuroboError::TomlParse(e.to_string())
    }
}

impl From<toml::ser::Error> for OuroboError {
    fn from(e: toml::ser::Error) -> Self {
        OuroboError::TomlSerialize(e.to_string())
    }
}

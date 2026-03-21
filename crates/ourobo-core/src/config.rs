use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub watches: Vec<WatchConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonConfig {
    pub ipc_path: PathBuf,
    pub debounce_ms: u64,
    pub log_file: Option<PathBuf>,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchConfig {
    pub id: String,
    pub label: String,
    pub source: PathBuf,
    pub target: TargetConfig,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TargetConfig {
    #[serde(rename = "local")]
    Local { path: PathBuf },
    #[serde(rename = "smb")]
    Smb {
        host: String,
        share: String,
        path: String,
        username: String,
    },
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            ipc_path: default_ipc_path(),
            debounce_ms: 500,
            log_file: None,
            log_level: "info".to_string(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            watches: Vec::new(),
        }
    }
}

pub fn default_ipc_path() -> PathBuf {
    #[cfg(unix)]
    {
        let base = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join(".ourobo")
            });
        base.join("ourobo.sock")
    }
    #[cfg(windows)]
    {
        PathBuf::from(r"\\.\pipe\ourobo-backup")
    }
}

pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            eprintln!("WARNING: could not determine config directory, falling back to current directory");
            PathBuf::from(".")
        })
        .join("ourobo")
        .join("config.toml")
}

impl AppConfig {
    pub fn load(path: &Path) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::OuroboError::ConfigNotFound(path.to_path_buf())
            } else {
                crate::OuroboError::Config(format!("failed to read {}: {e}", path.display()))
            }
        })?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load config from path, or return defaults if the file doesn't exist.
    /// Creates a default config file on first run.
    pub fn load_or_default(path: &Path) -> crate::Result<Self> {
        match Self::load(path) {
            Ok(config) => Ok(config),
            Err(crate::OuroboError::ConfigNotFound(_)) => {
                let config = Self::default();
                config.save(path)?;
                Ok(config)
            }
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AppConfig {
        AppConfig {
            daemon: DaemonConfig {
                ipc_path: PathBuf::from("/tmp/ourobo.sock"),
                debounce_ms: 500,
                log_file: None,
                log_level: "info".to_string(),
            },
            watches: vec![WatchConfig {
                id: "docs".to_string(),
                label: "My Documents".to_string(),
                source: PathBuf::from("/home/user/Documents"),
                target: TargetConfig::Local {
                    path: PathBuf::from("/mnt/backup/Documents"),
                },
                exclude: vec!["*.tmp".to_string(), ".DS_Store".to_string()],
                enabled: true,
            }],
        }
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = sample_config();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_deserialize_example_config() {
        let toml_str = r#"
[daemon]
ipc_path = "/tmp/ourobo.sock"
debounce_ms = 300
log_level = "debug"

[[watches]]
id = "photos"
label = "Photos"
source = "/home/user/Photos"
exclude = ["*.tmp"]
enabled = true

[watches.target]
type = "local"
path = "/backup/Photos"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.debounce_ms, 300);
        assert_eq!(config.daemon.log_level, "debug");
        assert_eq!(config.watches.len(), 1);
        assert_eq!(config.watches[0].id, "photos");
        assert_eq!(config.watches[0].label, "Photos");
        assert!(config.watches[0].enabled);
        assert_eq!(
            config.watches[0].target,
            TargetConfig::Local {
                path: PathBuf::from("/backup/Photos")
            }
        );
    }

    #[test]
    fn test_default_daemon_config() {
        let daemon = DaemonConfig::default();
        assert_eq!(daemon.debounce_ms, 500);
        assert_eq!(daemon.log_level, "info");
        assert!(daemon.log_file.is_none());
    }

    #[test]
    fn test_config_with_smb_target() {
        let toml_str = r#"
[daemon]
ipc_path = "/tmp/ourobo.sock"
debounce_ms = 500
log_level = "info"

[[watches]]
id = "remote"
label = "Remote Share"
source = "/home/user/work"
exclude = []
enabled = true

[watches.target]
type = "smb"
host = "192.168.1.100"
share = "backups"
path = "/work"
username = "user"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        match &config.watches[0].target {
            TargetConfig::Smb {
                host,
                share,
                username,
                ..
            } => {
                assert_eq!(host, "192.168.1.100");
                assert_eq!(share, "backups");
                assert_eq!(username, "user");
            }
            _ => panic!("expected SMB target"),
        }
    }

    #[test]
    fn test_config_save_and_load() {
        let config = sample_config();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        config.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn test_config_load_not_found() {
        let result = AppConfig::load(Path::new("/nonexistent/config.toml"));
        assert!(matches!(
            result.unwrap_err(),
            crate::OuroboError::ConfigNotFound(_)
        ));
    }

    #[test]
    fn test_config_load_or_default_rejects_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is [not valid { toml").unwrap();

        let result = AppConfig::load_or_default(&path);
        assert!(matches!(
            result.unwrap_err(),
            crate::OuroboError::TomlParse(_)
        ));
    }

    #[test]
    fn test_config_load_or_default_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");
        assert!(!path.exists());

        let config = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(config, AppConfig::default());
        assert!(path.exists());

        // Second call loads the saved file
        let config2 = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(config, config2);
    }

    #[test]
    fn test_config_load_or_default_uses_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let custom = sample_config();
        custom.save(&path).unwrap();

        let loaded = AppConfig::load_or_default(&path).unwrap();
        assert_eq!(loaded, custom);
    }

    #[test]
    fn test_default_enabled() {
        let toml_str = r#"
[daemon]
ipc_path = "/tmp/ourobo.sock"
debounce_ms = 500
log_level = "info"

[[watches]]
id = "test"
label = "Test"
source = "/tmp/src"

[watches.target]
type = "local"
path = "/tmp/dest"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.watches[0].enabled);
    }
}

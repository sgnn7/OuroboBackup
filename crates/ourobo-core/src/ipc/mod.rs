pub mod client;
pub mod server;

use crate::config::WatchConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", content = "data")]
pub enum IpcCommand {
    Ping,
    Status,
    AddWatch(WatchConfig),
    RemoveWatch { id: String },
    ListWatches,
    SetWatchEnabled { id: String, enabled: bool },
    TriggerBackup { id: String },
    ReloadConfig,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "data")]
pub enum IpcResponse {
    #[serde(rename = "ok")]
    Ok(ResponseData),
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseData {
    Pong,
    DaemonStatus(DaemonStatus),
    WatchList(Vec<WatchStatus>),
    WatchAdded { id: String },
    WatchRemoved { id: String },
    WatchUpdated { id: String },
    BackupTriggered { id: String },
    ConfigReloaded,
    ShuttingDown,
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonStatus {
    pub uptime_secs: u64,
    pub active_watches: usize,
    pub total_files_backed_up: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchStatus {
    pub config: WatchConfig,
    pub files_backed_up: u64,
    pub last_backup: Option<String>,
    pub last_error: Option<String>,
    pub is_watching: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_command_ping_roundtrip() {
        let cmd = IpcCommand::Ping;
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn test_command_add_watch_roundtrip() {
        let cmd = IpcCommand::AddWatch(WatchConfig {
            id: "test".to_string(),
            label: "Test Watch".to_string(),
            source: PathBuf::from("/src"),
            target: crate::config::TargetConfig::Local {
                path: PathBuf::from("/dest"),
            },
            exclude: vec!["*.tmp".to_string()],
            enabled: true,
        });
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn test_command_remove_watch_roundtrip() {
        let cmd = IpcCommand::RemoveWatch {
            id: "abc".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn test_command_set_watch_enabled_roundtrip() {
        let cmd = IpcCommand::SetWatchEnabled {
            id: "x".to_string(),
            enabled: false,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);
    }

    #[test]
    fn test_response_ok_pong_roundtrip() {
        let resp = IpcResponse::Ok(ResponseData::Pong);
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_response_error_roundtrip() {
        let resp = IpcResponse::Error {
            message: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_response_daemon_status_roundtrip() {
        let resp = IpcResponse::Ok(ResponseData::DaemonStatus(DaemonStatus {
            uptime_secs: 3600,
            active_watches: 2,
            total_files_backed_up: 150,
            last_error: None,
        }));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_response_watch_list_roundtrip() {
        let resp = IpcResponse::Ok(ResponseData::WatchList(vec![WatchStatus {
            config: WatchConfig {
                id: "w1".to_string(),
                label: "Watch 1".to_string(),
                source: PathBuf::from("/src"),
                target: crate::config::TargetConfig::Local {
                    path: PathBuf::from("/dest"),
                },
                exclude: vec![],
                enabled: true,
            },
            files_backed_up: 42,
            last_backup: Some("2026-01-01T00:00:00Z".to_string()),
            last_error: None,
            is_watching: true,
        }]));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_all_commands_serialize() {
        let commands = vec![
            IpcCommand::Ping,
            IpcCommand::Status,
            IpcCommand::ListWatches,
            IpcCommand::ReloadConfig,
            IpcCommand::Shutdown,
            IpcCommand::TriggerBackup {
                id: "x".to_string(),
            },
        ];
        for cmd in commands {
            let json = serde_json::to_string(&cmd).unwrap();
            let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
            assert_eq!(cmd, parsed);
        }
    }

    #[test]
    fn test_ping_json_format() {
        let cmd = IpcCommand::Ping;
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(json, r#"{"cmd":"Ping"}"#);
    }
}

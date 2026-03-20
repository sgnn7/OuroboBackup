# OuroboBackup — Implementation Plan

## Context

Build a cross-platform (macOS → Linux → Windows) file backup application in Rust that watches directories for changes and copies modified files to a target location. The app uses a daemon + thin-client architecture with CLI and GUI interfaces. Starting with macOS, local filesystem backend, and copy-on-change strategy, with SMB and more complex strategies planned for later.

**Key decisions:**
- Language: Rust
- GUI: egui/eframe (pure Rust, minimal OS deps)
- SMB: smb-rs (pure Rust, future phase)
- Strategy: Copy-on-change now, trait-based for extensibility
- Architecture: Background daemon + CLI/GUI thin clients via IPC (Unix socket / named pipe)
- Config: TOML via serde
- File watching: `notify` + `notify-debouncer-mini`
- TDD throughout

---

## Workspace Structure

```
OuroboBackup/
├── Cargo.toml              # Workspace root
├── README.md
├── PLAN.md                 # Copy of this plan
├── .gitignore
├── config.example.toml
├── crates/
│   ├── ourobo-core/        # Shared library: config, backends, strategies, IPC, watcher, engine
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── config.rs
│   │       ├── backend/
│   │       │   ├── mod.rs          # BackupBackend trait
│   │       │   ├── local.rs        # LocalFsBackend
│   │       │   └── smb.rs          # SmbBackend (future)
│   │       ├── strategy/
│   │       │   ├── mod.rs          # BackupStrategy trait
│   │       │   └── copy_on_change.rs
│   │       ├── watcher.rs
│   │       ├── ipc/
│   │       │   ├── mod.rs          # IpcCommand, IpcResponse types
│   │       │   ├── client.rs       # IpcClient (used by CLI/GUI)
│   │       │   └── server.rs       # IpcServer (used by daemon)
│   │       └── engine.rs           # BackupEngine orchestrator
│   ├── ourobo-daemon/      # Background daemon binary
│   │   └── src/
│   │       ├── main.rs
│   │       └── daemon.rs
│   ├── ourobo-cli/          # CLI thin client binary
│   │   └── src/
│   │       └── main.rs
│   ├── ourobo-gui/          # egui GUI thin client binary
│   │   └── src/
│   │       ├── main.rs
│   │       └── app.rs
│   └── ourobo-tray/         # macOS menu bar tray icon
│       └── src/
│           └── main.rs
```

## Key Dependencies

| Crate | Purpose |
|---|---|
| `notify` + `notify-debouncer-mini` | Cross-platform file watching |
| `tokio` | Async runtime (daemon, IPC) |
| `serde` + `serde_json` + `toml` | Serialization (config, IPC) |
| `clap` (derive) | CLI argument parsing |
| `eframe` / `egui` | GUI |
| `thiserror` / `anyhow` | Error handling |
| `tracing` + `tracing-subscriber` | Logging |
| `chrono`, `uuid` | Timestamps, watch IDs |
| `dirs` | Cross-platform config/home dirs |
| `globset` | Exclude pattern matching |
| `async-trait` | Async trait support |
| `tray-icon` + `muda` | System tray icon and menu |
| `tempfile`, `mockall` | Testing |

## Core Traits

### `BackupBackend` (in `backend/mod.rs`)
```rust
#[async_trait]
pub trait BackupBackend: Send + Sync {
    async fn copy_file(&self, source: &Path, dest_relative: &Path) -> Result<()>;
    async fn file_meta(&self, dest_relative: &Path) -> Result<RemoteFileMeta>;
    async fn create_dir_all(&self, dest_relative: &Path) -> Result<()>;
    async fn delete_file(&self, dest_relative: &Path) -> Result<()>;
    fn name(&self) -> &str;
}
```

### `BackupStrategy` (in `strategy/mod.rs`)
```rust
#[async_trait]
pub trait BackupStrategy: Send + Sync {
    async fn handle_event(
        &self, event: &FileEvent, watch_source_root: &Path, backend: &dyn BackupBackend,
    ) -> Result<BackupResult>;
    fn name(&self) -> &str;
}
```

## IPC Protocol

JSON-over-newline via Unix domain socket (macOS/Linux) or named pipe (Windows).

**Commands:** `Ping`, `Status`, `AddWatch`, `RemoveWatch`, `ListWatches`, `SetWatchEnabled`, `TriggerBackup`, `ReloadConfig`, `Shutdown`

**Responses:** `Ok(ResponseData)` or `Error { message }` — where `ResponseData` includes `Pong`, `DaemonStatus`, `WatchList`, etc.

## Config Format (TOML)

```toml
[daemon]
ipc_path = "~/.ourobo/ourobo.sock"
debounce_ms = 500
log_level = "info"

[[watches]]
id = "documents"
label = "My Documents"
source = "/Users/sg/Documents"
exclude = ["*.tmp", ".DS_Store"]
enabled = true

[watches.target]
type = "local"
path = "/Volumes/Backup/Documents"
```

## Implementation Phases (TDD)

### Phase 1: Workspace + Core Types
1. Create workspace `Cargo.toml`, all crate `Cargo.toml` files, `.gitignore`
2. Implement `error.rs` — unified error types
3. **TDD**: Write config serialization tests → implement `config.rs`
4. **TDD**: Write IPC message serialization tests → implement `ipc/mod.rs` types
5. Update `README.md` with project overview

### Phase 2: Backends + Strategies
6. **TDD**: Write `LocalFsBackend` tests (using `tempfile`) → implement `backend/local.rs`
7. **TDD**: Write `CopyOnChange` tests (using `MockBackupBackend`) → implement `strategy/copy_on_change.rs`
8. Update `README.md`

### Phase 3: Watcher + IPC Transport
9. Implement `watcher.rs` with integration test (watch a tempdir, write file, assert event received)
10. **TDD**: Write IPC client/server roundtrip tests → implement `ipc/client.rs` + `ipc/server.rs`
11. Update `README.md`

### Phase 4: Engine + Daemon
12. **TDD**: Write engine tests → implement `engine.rs`
13. Implement `ourobo-daemon` (config loading, engine init, IPC server, signal handling)
14. Integration test: start daemon, send commands via IpcClient
15. Update `README.md`

### Phase 5: CLI
16. Implement `ourobo-cli` with all subcommands via clap derive
17. Test CLI argument parsing
18. Manual end-to-end test against running daemon
19. Update `README.md`

### Phase 6: GUI
20. Implement `ourobo-gui` — connection status, watch list, add/remove, settings
21. Manual testing against running daemon
22. Update `README.md`

### Phase 7: Linux + Windows (future)
23. Verify/fix on Linux (notify uses inotify — should mostly work)
24. Windows: named pipe IPC, verify notify with ReadDirectoryChangesW
25. Platform-specific packaging (systemd unit, Windows service)

### Phase 8: SMB Backend (future)
26. Implement `backend/smb.rs` using `smb-rs`
27. Credential management (OS keychain)

## Key Test Cases (written before implementation)

**Config:** roundtrip serialization, example TOML parsing, defaults, SMB target variant
**IPC messages:** roundtrip for each command/response variant, JSON wire format
**LocalFsBackend:** copy creates dest + parent dirs, file_meta existing/nonexistent, delete
**CopyOnChange:** Modified→copy_file called, Created→copy_file called, Deleted→delete_file called, path outside root errors
**IPC transport:** ping/pong roundtrip, multiple commands, concurrent clients
**Engine:** add/list/remove watch, duplicate ID rejected

## Verification

1. `cargo test --workspace` — all unit and integration tests pass
2. Start daemon: `cargo run -p ourobo-daemon`
3. CLI check: `cargo run -p ourobo-cli -- ping` → "Pong"
4. CLI add watch: `cargo run -p ourobo-cli -- add --label test --source /tmp/src --target /tmp/dest`
5. Create file in `/tmp/src/`, verify it appears in `/tmp/dest/`
6. CLI status: `cargo run -p ourobo-cli -- status` → shows active watch
7. GUI: `cargo run -p ourobo-gui` → shows daemon connected, watch listed

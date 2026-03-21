# OuroboBackup

Cross-platform file backup tool that watches directories and copies changed files to a target location.

## Architecture

- **ourobo-core** — shared library: config, backends, strategies, IPC, file watcher, backup engine
- **ourobo-daemon** — background daemon that watches files and performs backups
- **ourobo-cli** — command-line client to control the daemon
- **ourobo-gui** — egui-based graphical client
- **ourobo-tray** — macOS menu bar tray icon (via `tray-icon`)

The daemon runs in the background and communicates with CLI/GUI clients via IPC (Unix domain socket on macOS/Linux, named pipe on Windows). It handles SIGINT, SIGTERM, and IPC shutdown commands for graceful shutdown with socket cleanup.

## Backup Strategy

Currently implements **copy-on-change**: files are copied to the target immediately when modified. The architecture supports pluggable strategies for future extensibility (versioned snapshots, incremental backups, etc.).

## Backends

- **Local filesystem** — copies to a local or mounted directory
- **SMB** — planned (pure Rust via smb-rs)

## Features

- File watching with configurable debounce (via `notify`)
- Glob-based exclude patterns (e.g., `*.tmp`, `.DS_Store`, `target/**`)
- Daemon + thin-client architecture with IPC
- CLI, GUI, and system tray clients
- TOML configuration
- Graceful shutdown (SIGINT, SIGTERM, IPC shutdown command)
- Path traversal protection in local filesystem backend

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run tests (55 tests)
cargo test --workspace

# Start the daemon (requires config at ~/.config/ourobo/config.toml)
cargo run -p ourobo-daemon

# CLI usage
cargo run -p ourobo-cli -- ping
cargo run -p ourobo-cli -- status
cargo run -p ourobo-cli -- add --id docs --label "Documents" --source ~/Documents --target /backup/Documents
cargo run -p ourobo-cli -- list
cargo run -p ourobo-cli -- remove docs
cargo run -p ourobo-cli -- shutdown

# GUI (connects to running daemon)
cargo run -p ourobo-gui

# System tray icon (macOS menu bar)
cargo run -p ourobo-tray
```

## GUI

The graphical interface connects to a running daemon and provides:

- Connection status indicator with configurable socket path
- Live daemon status (uptime, active watches, files backed up)
- Watch list with per-watch controls (backup now, remove)
- Add Watch dialog (ID, label, source path, target path)
- Confirmation dialogs for destructive actions
- Auto-refresh every 2 seconds
- Toast notifications with auto-dismiss

## System Tray

The system tray app (`ourobo-tray`) sits in the macOS menu bar and provides:

- Green status icon indicating the daemon is reachable
- Live status display (watch count, files backed up)
- Quick access to launch the GUI
- Quit option
- Polls daemon status every 5 seconds via IPC

## Configuration

Config file: `~/.config/ourobo/config.toml`

```toml
[daemon]
ipc_path = "~/.ourobo/ourobo.sock"
debounce_ms = 500
log_level = "info"

[[watches]]
id = "documents"
label = "My Documents"
source = "/Users/you/Documents"
exclude = ["*.tmp", ".DS_Store", "Thumbs.db"]
enabled = true

[watches.target]
type = "local"
path = "/Volumes/Backup/Documents"
```

See `config.example.toml` for a full example.

### Exclude Patterns

Watches support glob-based exclude patterns. Patterns are matched against both the filename and the relative path from the watch source:

- `*.tmp` — exclude all `.tmp` files
- `.DS_Store` — exclude by exact filename
- `target/**` — exclude an entire directory tree
- `node_modules/**` — exclude nested dependency directories

## CLI Reference

| Command | Description |
|---|---|
| `ourobo ping` | Check if daemon is running |
| `ourobo status` | Show daemon uptime, watch count, files backed up |
| `ourobo list` | List all watches with status |
| `ourobo add --id ID --label LABEL --source PATH --target PATH` | Add a new watch |
| `ourobo remove ID` | Remove a watch |
| `ourobo enable ID` | Enable a watch |
| `ourobo disable ID` | Disable a watch |
| `ourobo backup ID` | Trigger immediate backup |
| `ourobo reload` | Reload daemon configuration |
| `ourobo shutdown` | Shut down daemon gracefully |

Use `--socket PATH` or `OUROBO_SOCKET` env var to specify a custom daemon socket path.

## Packaging (macOS)

```bash
# Build a .dmg containing all binaries, config example, and .app bundles
./scripts/build-dmg.sh
```

Output: `target/OuroboBackup-0.1.0.dmg`

The script builds release binaries, optionally creates `.app` bundles via `cargo-bundle` (install with `cargo install cargo-bundle`), and packages everything into a compressed DMG using `hdiutil`.

## Development

```bash
# Run all tests (55 across workspace)
cargo test --workspace

# Run tests for a specific crate
cargo test -p ourobo-core
cargo test -p ourobo-cli

# Check without building
cargo check --workspace
```

## License

MIT

# OuroboBackup

Cross-platform file backup tool that watches directories and copies changed files to a target location.

## Architecture

- **ourobo-core** — shared library: config, backends, strategies, IPC, file watcher, backup engine
- **ourobo-daemon** — background daemon that watches files and performs backups
- **ourobo-cli** — command-line client to control the daemon
- **ourobo-gui** — egui-based graphical client

The daemon runs in the background and communicates with CLI/GUI clients via IPC (Unix domain socket on macOS/Linux, named pipe on Windows).

## Backup Strategy

Currently implements **copy-on-change**: files are copied to the target immediately when modified. The architecture supports pluggable strategies for future extensibility (versioned snapshots, incremental backups, etc.).

## Backends

- **Local filesystem** — copies to a local or mounted directory
- **SMB** — planned (pure Rust via smb-rs)

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Start the daemon (requires config at ~/.config/ourobo/config.toml)
cargo run -p ourobo-daemon

# CLI usage
cargo run -p ourobo-cli -- ping
cargo run -p ourobo-cli -- status
cargo run -p ourobo-cli -- add --id docs --label "Documents" --source ~/Documents --target /backup/Documents
cargo run -p ourobo-cli -- list
cargo run -p ourobo-cli -- remove docs

# GUI (connects to running daemon)
cargo run -p ourobo-gui
```

## GUI

The graphical interface connects to a running daemon and provides:

- Connection status indicator with configurable socket path
- Live daemon status (uptime, active watches, files backed up)
- Watch list with per-watch controls (backup now, remove)
- Add Watch dialog (ID, label, source path, target path)
- Confirmation dialogs for destructive actions
- Auto-refresh every 2 seconds

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
exclude = ["*.tmp", ".DS_Store"]
enabled = true

[watches.target]
type = "local"
path = "/Volumes/Backup/Documents"
```

See `config.example.toml` for a full example.

## Development

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p ourobo-core

# Check without building
cargo check --workspace
```

## License

MIT

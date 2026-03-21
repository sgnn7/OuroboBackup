# AGENTS.md

## Project Overview

OuroboBackup is a cross-platform file backup tool written in Rust. It watches directories for changes and copies modified files to a target location using a daemon + thin-client architecture.

## Build & Test

```bash
cargo build --workspace       # Build everything
cargo test --workspace        # Run all tests (38 unit + integration tests)
cargo test -p ourobo-core     # Run core library tests only
cargo check --workspace       # Type-check without building
```

## Workspace Structure

Four crates in `crates/`:

- **ourobo-core** — Shared library. All domain logic lives here: config, error types, backend trait + local filesystem impl, strategy trait + copy-on-change impl, file watcher, IPC protocol/client/server, backup engine.
- **ourobo-daemon** — Background daemon binary. Loads config, starts engine, serves IPC.
- **ourobo-cli** — CLI thin client. Sends IPC commands to daemon via Unix socket.
- **ourobo-gui** — egui-based GUI thin client. Connects to daemon for status/control.

## Key Architectural Patterns

- **Trait-based backends**: `BackupBackend` trait in `crates/ourobo-core/src/backend/mod.rs`. Implement this for new storage targets. `LocalFsBackend` validates paths against traversal attacks.
- **Trait-based strategies**: `BackupStrategy` trait in `crates/ourobo-core/src/strategy/mod.rs`. Currently only `CopyOnChange`; designed for future extensibility.
- **IPC protocol**: JSON-over-newline on Unix domain sockets. Types in `crates/ourobo-core/src/ipc/mod.rs`. Server caps messages at 1 MB.
- **Config**: TOML via serde. Models in `crates/ourobo-core/src/config.rs`. Example in `config.example.toml`.

## Conventions

- **TDD**: Write tests before implementation. Tests live in `#[cfg(test)] mod tests` blocks alongside their modules.
- **Mocking**: Use `mockall` for trait mocking (e.g., `MockBackupBackend`). Use `tempfile` for filesystem tests.
- **macOS symlinks**: File watcher tests must canonicalize tempdir paths to handle `/var` → `/private/var`.
- **Error handling**: Use `OuroboError` (thiserror) in core, `anyhow` in binaries.
- **Async**: tokio runtime. Backend trait uses `async-trait`.
- **Platform gating**: Use `#[cfg(unix)]` / `#[cfg(windows)]` for IPC transport. Windows returns explicit "not yet supported" errors.
- **No unused code warnings**: Keep imports clean. Remove `BackupAction::Failed` and similar dead variants rather than leaving them.
- **Update README.md** when adding features or changing usage.
- **Stage changes but let the user commit.**
- **Granular commits**: One logical change per commit. Include related docs updates (README, PLAN.md) in the same commit as the code they describe. Only use separate commits for truly independent changes (e.g., "add exclude filtering" vs "add signal handling").

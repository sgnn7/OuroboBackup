use anyhow::Result;
use clap::{Parser, Subcommand};
use ourobo_core::config::{default_ipc_path, TargetConfig, WatchConfig};
use ourobo_core::ipc::client::IpcClient;
use ourobo_core::ipc::{IpcCommand, IpcResponse, ResponseData};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ourobo", about = "OuroboBackup — file backup CLI")]
struct Cli {
    /// Path to daemon socket
    #[arg(long, env = "OUROBO_SOCKET")]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check if daemon is running
    Ping,
    /// Show daemon status
    Status,
    /// Add a watch
    Add {
        /// Unique watch ID
        #[arg(long)]
        id: String,
        /// Human-readable label
        #[arg(long)]
        label: String,
        /// Source directory to watch
        #[arg(long)]
        source: PathBuf,
        /// Target directory for backups
        #[arg(long)]
        target: PathBuf,
    },
    /// Remove a watch
    Remove {
        /// Watch ID to remove
        id: String,
    },
    /// List all watches
    List,
    /// Enable a watch
    Enable {
        /// Watch ID
        id: String,
    },
    /// Disable a watch
    Disable {
        /// Watch ID
        id: String,
    },
    /// Trigger immediate backup
    Backup {
        /// Watch ID
        id: String,
    },
    /// Reload daemon configuration
    Reload,
    /// Shut down daemon
    Shutdown,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let socket_path = cli.socket.unwrap_or_else(default_ipc_path);

    let mut client = IpcClient::connect(&socket_path).await?;

    let cmd = match cli.command {
        Commands::Ping => IpcCommand::Ping,
        Commands::Status => IpcCommand::Status,
        Commands::Add {
            id,
            label,
            source,
            target,
        } => IpcCommand::AddWatch(WatchConfig {
            id,
            label,
            source,
            target: TargetConfig::Local { path: target },
            exclude: vec![],
            enabled: true,
        }),
        Commands::Remove { id } => IpcCommand::RemoveWatch { id },
        Commands::List => IpcCommand::ListWatches,
        Commands::Enable { id } => IpcCommand::SetWatchEnabled { id, enabled: true },
        Commands::Disable { id } => IpcCommand::SetWatchEnabled { id, enabled: false },
        Commands::Backup { id } => IpcCommand::TriggerBackup { id },
        Commands::Reload => IpcCommand::ReloadConfig,
        Commands::Shutdown => IpcCommand::Shutdown,
    };

    let response = client.send(cmd).await?;

    match response {
        IpcResponse::Ok(data) => print_response(data),
        IpcResponse::Error { message } => {
            eprintln!("Error: {message}");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_response(data: ResponseData) {
    match data {
        ResponseData::Pong => println!("Pong"),
        ResponseData::DaemonStatus(s) => {
            println!("Uptime:  {}s", s.uptime_secs);
            println!("Watches: {}", s.active_watches);
            println!("Backed:  {} files", s.total_files_backed_up);
            if let Some(err) = s.last_error {
                println!("Error:   {err}");
            }
        }
        ResponseData::WatchList(watches) => {
            if watches.is_empty() {
                println!("No watches configured.");
                return;
            }
            for w in watches {
                let status = if w.is_watching { "active" } else { "paused" };
                println!(
                    "[{}] {} — {} ({}, {} files)",
                    w.config.id,
                    w.config.label,
                    w.config.source.display(),
                    status,
                    w.files_backed_up
                );
            }
        }
        ResponseData::WatchAdded { id } => println!("Watch added: {id}"),
        ResponseData::WatchRemoved { id } => println!("Watch removed: {id}"),
        ResponseData::WatchUpdated { id } => println!("Watch updated: {id}"),
        ResponseData::BackupTriggered { id } => println!("Backup triggered: {id}"),
        ResponseData::ConfigReloaded => println!("Config reloaded."),
        ResponseData::ShuttingDown => println!("Daemon shutting down."),
        ResponseData::Empty => {}
    }
}

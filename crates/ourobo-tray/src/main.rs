use anyhow::Result;
use muda::{accelerator::Accelerator, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use ourobo_core::config::default_ipc_path;
use ourobo_core::ipc::client::IpcClient;
use ourobo_core::ipc::{IpcCommand, IpcResponse, ResponseData};
use std::sync::mpsc;
use tray_icon::menu::MenuId;
use tray_icon::{Icon, TrayIconBuilder};

const STATUS_ID: &str = "status";
const OPEN_GUI_ID: &str = "open_gui";
const QUIT_ID: &str = "quit";

enum TrayUpdate {
    Status(String),
    Error(String),
}

fn build_icon() -> Icon {
    // 16x16 RGBA: simple green circle on transparent background
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = 6.0f32;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist <= radius {
                rgba[idx] = 80;     // R
                rgba[idx + 1] = 200; // G
                rgba[idx + 2] = 80;  // B
                rgba[idx + 3] = 255; // A
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("failed to create icon")
}

fn main() -> Result<()> {
    let (update_tx, update_rx) = mpsc::channel::<TrayUpdate>();

    // Background thread: poll daemon status periodically
    let socket_path = default_ipc_path();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = update_tx.send(TrayUpdate::Error(format!("Runtime error: {e}")));
                return;
            }
        };
        rt.block_on(async {
            loop {
                match IpcClient::connect(&socket_path).await {
                    Ok(mut client) => {
                        match client.send(IpcCommand::Status).await {
                            Ok(IpcResponse::Ok(ResponseData::DaemonStatus(s))) => {
                                let _ = update_tx.send(TrayUpdate::Status(format!(
                                    "{} watches, {} files backed up",
                                    s.active_watches, s.total_files_backed_up
                                )));
                            }
                            Ok(IpcResponse::Ok(_)) => {
                                let _ = update_tx.send(TrayUpdate::Status("Connected".to_string()));
                            }
                            Ok(IpcResponse::Error { message }) => {
                                let _ = update_tx.send(TrayUpdate::Error(message));
                            }
                            Err(e) => {
                                let _ = update_tx.send(TrayUpdate::Error(format!("IPC error: {e}")));
                            }
                        }
                    }
                    Err(_) => {
                        let _ = update_tx.send(TrayUpdate::Error("Daemon not running".to_string()));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    });

    // Build menu
    let status_item = MenuItem::with_id(STATUS_ID, "Connecting...", false, None::<Accelerator>);
    let open_gui_item = MenuItem::with_id(OPEN_GUI_ID, "Open GUI", true, None::<Accelerator>);
    let quit_item = MenuItem::with_id(QUIT_ID, "Quit", true, None::<Accelerator>);

    let menu = Menu::new();
    menu.append(&status_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&open_gui_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let _tray = TrayIconBuilder::new()
        .with_icon(build_icon())
        .with_menu(Box::new(menu))
        .with_tooltip("OuroboBackup")
        .build()?;

    let menu_rx = MenuEvent::receiver();

    // Main event loop (must run on main thread for macOS)
    loop {
        // Process menu events
        if let Ok(event) = menu_rx.try_recv() {
            if event.id() == &MenuId::new(QUIT_ID) {
                break;
            }
            if event.id() == &MenuId::new(OPEN_GUI_ID) {
                // Launch GUI as a separate process
                if let Err(e) = std::process::Command::new("cargo")
                    .args(["run", "-p", "ourobo-gui"])
                    .spawn()
                {
                    eprintln!("failed to launch GUI: {e}");
                }
            }
        }

        // Process status updates
        if let Ok(update) = update_rx.try_recv() {
            match update {
                TrayUpdate::Status(msg) => {
                    status_item.set_text(&msg);
                }
                TrayUpdate::Error(msg) => {
                    status_item.set_text(&msg);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

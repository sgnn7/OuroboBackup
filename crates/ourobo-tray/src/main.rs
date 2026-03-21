use muda::{accelerator::Accelerator, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use ourobo_core::config::default_ipc_path;
use ourobo_core::ipc::client::IpcClient;
use ourobo_core::ipc::{IpcCommand, IpcResponse, ResponseData};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::MenuId;
use tray_icon::{Icon, TrayIconBuilder};

#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

enum TrayUpdate {
    Status(String),
    Error(String),
}

fn build_icon() -> Icon {
    // 22x22 monochrome outline of a storage safe for the menu bar.
    // macOS menu bar icons should be simple, high-contrast outlines.
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    let set = |rgba: &mut Vec<u8>, x: u32, y: u32, a: u8| {
        if x < size && y < size {
            let idx = ((y * size + x) * 4) as usize;
            // White with variable alpha — macOS template images use white;
            // the system inverts automatically for light/dark menu bars
            rgba[idx] = 255;
            rgba[idx + 1] = 255;
            rgba[idx + 2] = 255;
            rgba[idx + 3] = a;
        }
    };

    // Safe body: rounded rectangle outline (3,2) to (18,19), 2px thick
    let x0: u32 = 3;
    let y0: u32 = 2;
    let x1: u32 = 18;
    let y1: u32 = 19;
    let a = 255u8;

    // Top and bottom edges (2px thick)
    for x in x0..=x1 {
        set(&mut rgba, x, y0, a);
        set(&mut rgba, x, y0 + 1, a);
        set(&mut rgba, x, y1, a);
        set(&mut rgba, x, y1 - 1, a);
    }
    // Left and right edges (2px thick)
    for y in y0..=y1 {
        set(&mut rgba, x0, y, a);
        set(&mut rgba, x0 + 1, y, a);
        set(&mut rgba, x1, y, a);
        set(&mut rgba, x1 - 1, y, a);
    }

    // Dial circle in center (radius ~3.5px, thicker ring)
    let cx = 10.5f32;
    let cy = 10.5f32;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            // Outer ring of dial (thicker)
            if (dist - 3.5).abs() < 1.0 {
                set(&mut rgba, x, y, a);
            }
            // Inner dot (larger)
            if dist < 1.5 {
                set(&mut rgba, x, y, a);
            }
        }
    }

    // Handle on right side (2px wide vertical bar)
    for y in 8..=13 {
        set(&mut rgba, 16, y, a);
        set(&mut rgba, 17, y, a);
    }

    // Hinges on left side (2px wide)
    for x in 4..=5 {
        for y in 5..=7 {
            set(&mut rgba, x, y, a);
        }
        for y in 14..=16 {
            set(&mut rgba, x, y, a);
        }
    }

    Icon::from_rgba(rgba, size, size).expect("failed to create icon")
}

fn spawn_status_poller(tx: mpsc::Sender<TrayUpdate>) {
    let socket_path = default_ipc_path();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(TrayUpdate::Error(format!("Runtime error: {e}")));
                return;
            }
        };
        rt.block_on(async {
            loop {
                let update = match IpcClient::connect(&socket_path).await {
                    Ok(mut client) => match client.send(IpcCommand::Status).await {
                        Ok(IpcResponse::Ok(ResponseData::DaemonStatus(s))) => {
                            TrayUpdate::Status(format!(
                                "{} watches | {} files",
                                s.active_watches, s.total_files_backed_up
                            ))
                        }
                        Ok(IpcResponse::Error { message }) => TrayUpdate::Error(message),
                        Ok(_) => TrayUpdate::Status("Connected".to_string()),
                        Err(e) => TrayUpdate::Error(format!("{e}")),
                    },
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("Connection refused") || msg.contains("No such file") {
                            TrayUpdate::Error("No daemon".to_string())
                        } else {
                            TrayUpdate::Error(format!("Daemon error: {e}"))
                        }
                    }
                };
                if tx.send(update).is_err() {
                    return; // UI exited, stop polling
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    });
}

fn main() {
    let (update_tx, update_rx) = mpsc::channel::<TrayUpdate>();
    spawn_status_poller(update_tx);

    let mut event_loop = EventLoopBuilder::new().build();
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let status_item = MenuItem::with_id("status", "Connecting...", false, None::<Accelerator>);
    let open_gui = MenuItem::with_id("open_gui", "Open GUI", true, None::<Accelerator>);
    let quit = MenuItem::with_id("quit", "Quit", true, None::<Accelerator>);

    let menu = Menu::new();
    menu.append(&status_item).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&open_gui).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&quit).unwrap();

    let quit_id = MenuId::new("quit");
    let open_gui_id = MenuId::new("open_gui");

    let mut tray_icon: Option<tray_icon::TrayIcon> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(200));

        match event {
            Event::NewEvents(StartCause::Init) => {
                match TrayIconBuilder::new()
                    .with_icon(build_icon())
                    .with_menu(Box::new(menu.clone()))
                    .with_tooltip("OuroboBackup")
                    .with_menu_on_left_click(true)
                    .build()
                {
                    Ok(icon) => {
                        icon.set_icon_as_template(true);
                        tray_icon = Some(icon);
                    }
                    Err(e) => {
                        eprintln!("failed to create tray icon: {e}");
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
            }
            Event::NewEvents(_) => {
                let menu_rx = MenuEvent::receiver();
                while let Ok(event) = menu_rx.try_recv() {
                    if event.id == quit_id {
                        tray_icon.take();
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    if event.id == open_gui_id {
                        let gui_path = std::env::current_exe()
                            .ok()
                            .and_then(|p| p.parent().map(|d| d.join("ourobo-gui")));
                        match gui_path {
                            Some(path) => {
                                if let Err(e) = std::process::Command::new(&path).spawn() {
                                    eprintln!("failed to launch GUI at {}: {e}", path.display());
                                    status_item.set_text("GUI launch failed");
                                }
                            }
                            None => {
                                eprintln!("failed to resolve GUI path from current exe");
                                status_item.set_text("GUI launch failed");
                            }
                        }
                    }
                }

                while let Ok(update) = update_rx.try_recv() {
                    match update {
                        TrayUpdate::Status(msg) => status_item.set_text(&msg),
                        TrayUpdate::Error(msg) => {
                            status_item.set_text(&format!("⚠ {msg}"));
                        }
                    }
                }
            }
            _ => {}
        }
    });
}

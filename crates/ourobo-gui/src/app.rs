use eframe::egui;
use ourobo_core::config::{default_ipc_path, TargetConfig, WatchConfig};
use ourobo_core::ipc::client::IpcClient;
use ourobo_core::ipc::{
    DaemonStatus, IpcCommand, IpcResponse, ResponseData, WatchStatus,
};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

const COLOR_OK: egui::Color32 = egui::Color32::from_rgb(80, 200, 80);
const COLOR_ERR: egui::Color32 = egui::Color32::from_rgb(200, 80, 80);
const COLOR_IDLE: egui::Color32 = egui::Color32::from_rgb(180, 180, 80);
const TOAST_DURATION_SECS: u64 = 5;
const REFRESH_INTERVAL_SECS: u64 = 2;

/// Messages sent from UI thread to background IPC thread
enum UiRequest {
    Connect(PathBuf),
    FetchStatus,
    FetchWatches,
    AddWatch(WatchConfig),
    RemoveWatch(String),
    TriggerBackup(String),
}

/// Messages sent from background IPC thread back to UI
enum DaemonReply {
    Connected,
    Disconnected(String),
    Status(DaemonStatus),
    Watches(Vec<WatchStatus>),
    Info(String),
    Error(String),
}

struct AddWatchForm {
    id: String,
    label: String,
    source: String,
    target: String,
}

impl AddWatchForm {
    fn new() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            source: String::new(),
            target: String::new(),
        }
    }

    fn clear(&mut self) {
        self.id.clear();
        self.label.clear();
        self.source.clear();
        self.target.clear();
    }
}

pub struct OuroboApp {
    connected: bool,
    socket_path: String,
    daemon_status: Option<DaemonStatus>,
    watches: Vec<WatchStatus>,

    request_tx: mpsc::Sender<UiRequest>,
    reply_rx: mpsc::Receiver<DaemonReply>,

    toast: Option<(String, bool, Instant)>, // (message, is_error, when)
    show_add_dialog: bool,
    add_form: AddWatchForm,
    confirm_remove: Option<String>,
    last_refresh: Instant,
}

impl OuroboApp {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<UiRequest>();
        let (reply_tx, reply_rx) = mpsc::channel::<DaemonReply>();

        // Spawn background thread with tokio runtime for IPC
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = reply_tx.send(DaemonReply::Disconnected(
                        format!("Failed to start IPC runtime: {e}"),
                    ));
                    return;
                }
            };
            rt.block_on(ipc_worker(request_rx, reply_tx));
        });

        let socket_path = default_ipc_path().to_string_lossy().to_string();

        let mut app = Self {
            connected: false,
            socket_path,
            daemon_status: None,
            watches: Vec::new(),
            request_tx,
            reply_rx,
            toast: None,
            show_add_dialog: false,
            add_form: AddWatchForm::new(),
            confirm_remove: None,
            last_refresh: Instant::now(),
        };

        app.send_request(UiRequest::Connect(PathBuf::from(&app.socket_path)));
        app
    }

    fn send_request(&mut self, req: UiRequest) {
        if self.request_tx.send(req).is_err() {
            self.connected = false;
            self.set_toast("Background worker stopped. Restart the application.".to_string(), true);
        }
    }

    fn request_refresh(&mut self) {
        self.send_request(UiRequest::FetchStatus);
        self.send_request(UiRequest::FetchWatches);
    }

    fn set_toast(&mut self, msg: String, is_error: bool) {
        // Don't let info messages overwrite errors
        if let Some((_, true, _)) = &self.toast {
            if !is_error {
                return;
            }
        }
        self.toast = Some((msg, is_error, Instant::now()));
    }

    fn poll_replies(&mut self) {
        while let Ok(reply) = self.reply_rx.try_recv() {
            match reply {
                DaemonReply::Connected => {
                    self.connected = true;
                    self.set_toast("Connected".to_string(), false);
                    self.request_refresh();
                }
                DaemonReply::Disconnected(msg) => {
                    self.connected = false;
                    self.daemon_status = None;
                    self.watches.clear();
                    self.set_toast(msg, true);
                }
                DaemonReply::Status(status) => {
                    self.daemon_status = Some(status);
                }
                DaemonReply::Watches(watches) => {
                    self.watches = watches;
                }
                DaemonReply::Info(msg) => {
                    self.set_toast(msg, false);
                    self.request_refresh();
                }
                DaemonReply::Error(msg) => {
                    self.set_toast(msg, true);
                }
            }
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("OuroboBackup");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = if self.connected {
                    (COLOR_OK, "Connected")
                } else {
                    (COLOR_ERR, "Disconnected")
                };
                ui.colored_label(color, label);

                if !self.connected && ui.button("Connect").clicked() {
                    self.send_request(UiRequest::Connect(PathBuf::from(&self.socket_path)));
                }

                if self.connected && ui.button("Refresh").clicked() {
                    self.request_refresh();
                }
            });
        });
    }

    fn render_status(&self, ui: &mut egui::Ui) {
        if let Some(status) = &self.daemon_status {
            ui.horizontal(|ui| {
                ui.label(format!("Uptime: {}s", status.uptime_secs));
                ui.separator();
                ui.label(format!("Watches: {}", status.active_watches));
                ui.separator();
                ui.label(format!("Files backed up: {}", status.total_files_backed_up));
            });
        }
    }

    fn render_watches(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Watches");
            if ui.button("+ Add").clicked() {
                self.show_add_dialog = true;
                self.add_form.clear();
            }
        });
        ui.separator();

        if self.watches.is_empty() {
            ui.label("No watches configured.");
            return;
        }

        let watches_snapshot: Vec<_> = self.watches.clone();
        for w in &watches_snapshot {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let status_color = if w.is_watching { COLOR_OK } else { COLOR_IDLE };
                    ui.colored_label(status_color, "●");
                    ui.strong(&w.config.label);
                    ui.label(format!("({})", w.config.id));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Remove").clicked() {
                            self.confirm_remove = Some(w.config.id.clone());
                        }
                        if ui.button("Backup").clicked() {
                            self.send_request(UiRequest::TriggerBackup(w.config.id.clone()));
                        }
                    });
                });

                ui.label(format!("Source: {}", w.config.source.display()));
                match &w.config.target {
                    TargetConfig::Local { path } => {
                        ui.label(format!("Target: {}", path.display()));
                    }
                    TargetConfig::Smb { host, share, .. } => {
                        ui.label(format!("Target: smb://{host}/{share}"));
                    }
                }

                ui.horizontal(|ui| {
                    ui.label(format!("{} files backed up", w.files_backed_up));
                    if let Some(err) = &w.last_error {
                        ui.colored_label(COLOR_ERR, format!("Error: {err}"));
                    }
                });
            });
            ui.add_space(2.0);
        }
    }

    fn render_add_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_add_dialog;
        egui::Window::new("Add Watch")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("add_watch_grid").show(ui, |ui| {
                    ui.label("ID:");
                    ui.text_edit_singleline(&mut self.add_form.id);
                    ui.end_row();

                    ui.label("Label:");
                    ui.text_edit_singleline(&mut self.add_form.label);
                    ui.end_row();

                    ui.label("Source:");
                    ui.text_edit_singleline(&mut self.add_form.source);
                    ui.end_row();

                    ui.label("Target:");
                    ui.text_edit_singleline(&mut self.add_form.target);
                    ui.end_row();
                });

                ui.separator();
                ui.horizontal(|ui| {
                    let can_add = !self.add_form.id.is_empty()
                        && !self.add_form.source.is_empty()
                        && !self.add_form.target.is_empty();

                    if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                        let config = WatchConfig {
                            id: self.add_form.id.clone(),
                            label: if self.add_form.label.is_empty() {
                                self.add_form.id.clone()
                            } else {
                                self.add_form.label.clone()
                            },
                            source: PathBuf::from(&self.add_form.source),
                            target: TargetConfig::Local {
                                path: PathBuf::from(&self.add_form.target),
                            },
                            exclude: vec![],
                            enabled: true,
                        };
                        self.send_request(UiRequest::AddWatch(config));
                        self.show_add_dialog = false;
                    }

                    if ui.button("Cancel").clicked() {
                        self.show_add_dialog = false;
                    }
                });
            });
        self.show_add_dialog = open;
    }

    fn render_remove_confirm(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.confirm_remove.clone() {
            let mut open = true;
            egui::Window::new("Confirm Remove")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Remove watch \"{id}\"?"));
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() {
                            self.send_request(UiRequest::RemoveWatch(id));
                            self.confirm_remove = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_remove = None;
                        }
                    });
                });
            if !open {
                self.confirm_remove = None;
            }
        }
    }

    fn render_toast(&self, ui: &mut egui::Ui) {
        if let Some((msg, is_error, when)) = &self.toast {
            if when.elapsed() < std::time::Duration::from_secs(TOAST_DURATION_SECS) {
                let color = if *is_error { COLOR_ERR } else { COLOR_OK };
                ui.colored_label(color, msg.as_str());
            }
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.label("Socket:");
        ui.text_edit_singleline(&mut self.socket_path);
    }
}

impl eframe::App for OuroboApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_replies();

        // Periodic refresh when connected
        if self.connected
            && self.last_refresh.elapsed()
                >= std::time::Duration::from_secs(REFRESH_INTERVAL_SECS)
        {
            self.request_refresh();
            self.last_refresh = Instant::now();
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            self.render_header(ui);
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.render_settings(ui);
                ui.separator();
                self.render_toast(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_status(ui);
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_watches(ui);
            });
        });

        self.render_add_dialog(ctx);
        self.render_remove_confirm(ctx);

        ctx.request_repaint_after(std::time::Duration::from_secs(REFRESH_INTERVAL_SECS));
    }
}

async fn ipc_worker(rx: mpsc::Receiver<UiRequest>, tx: mpsc::Sender<DaemonReply>) {
    let mut client: Option<IpcClient> = None;

    loop {
        let request = match rx.recv() {
            Ok(r) => r,
            Err(_) => break,
        };

        match request {
            UiRequest::Connect(path) => {
                match IpcClient::connect(&path).await {
                    Ok(c) => {
                        client = Some(c);
                        if tx.send(DaemonReply::Connected).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        client = None;
                        if tx.send(DaemonReply::Disconnected(e.to_string())).is_err() {
                            return;
                        }
                    }
                }
            }
            UiRequest::FetchStatus
            | UiRequest::FetchWatches
            | UiRequest::AddWatch(_)
            | UiRequest::RemoveWatch(_)
            | UiRequest::TriggerBackup(_) => {
                let Some(c) = client.as_mut() else {
                    if tx.send(DaemonReply::Error("Not connected".to_string())).is_err() {
                        return;
                    }
                    continue;
                };

                let cmd = match &request {
                    UiRequest::FetchStatus => IpcCommand::Status,
                    UiRequest::FetchWatches => IpcCommand::ListWatches,
                    UiRequest::AddWatch(config) => IpcCommand::AddWatch(config.clone()),
                    UiRequest::RemoveWatch(id) => IpcCommand::RemoveWatch { id: id.clone() },
                    UiRequest::TriggerBackup(id) => IpcCommand::TriggerBackup { id: id.clone() },
                    UiRequest::Connect(_) => unreachable!(),
                };

                let reply = match c.send(cmd).await {
                    Ok(IpcResponse::Ok(data)) => match data {
                        ResponseData::DaemonStatus(s) => DaemonReply::Status(s),
                        ResponseData::WatchList(w) => DaemonReply::Watches(w),
                        ResponseData::WatchAdded { id } => {
                            DaemonReply::Info(format!("Watch added: {id}"))
                        }
                        ResponseData::WatchRemoved { id } => {
                            DaemonReply::Info(format!("Watch removed: {id}"))
                        }
                        ResponseData::WatchUpdated { id } => {
                            DaemonReply::Info(format!("Watch updated: {id}"))
                        }
                        ResponseData::BackupTriggered { id } => {
                            DaemonReply::Info(format!("Backup triggered: {id}"))
                        }
                        ResponseData::Pong => DaemonReply::Info("Pong".to_string()),
                        ResponseData::ConfigReloaded => {
                            DaemonReply::Info("Config reloaded".to_string())
                        }
                        ResponseData::ShuttingDown => {
                            client = None;
                            DaemonReply::Disconnected("Daemon is shutting down".to_string())
                        }
                        ResponseData::Empty => continue,
                    },
                    Ok(IpcResponse::Error { message }) => DaemonReply::Error(message),
                    Err(e) => {
                        client = None;
                        DaemonReply::Disconnected(format!("Connection lost: {e}"))
                    }
                };

                if tx.send(reply).is_err() {
                    return;
                }
            }
        }
    }
}

use eframe::egui;

pub struct OuroboApp {
    connected: bool,
    status_text: String,
}

impl OuroboApp {
    pub fn new() -> Self {
        Self {
            connected: false,
            status_text: "Not connected to daemon".to_string(),
        }
    }
}

impl eframe::App for OuroboApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OuroboBackup");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (color, label) = if self.connected {
                        (egui::Color32::GREEN, "Connected")
                    } else {
                        (egui::Color32::RED, "Disconnected")
                    };
                    ui.colored_label(color, label);
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(&self.status_text);
            ui.separator();
            ui.label("Watches will appear here once connected to the daemon.");
        });
    }
}

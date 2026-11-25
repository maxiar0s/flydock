use eframe::egui;
use flydock::lifecycle::{LifecycleController, LifecycleState};
use flydock::registration::{Registration, RegistrationStore, default_store_path};
use std::path::PathBuf;

struct FlyDockApp {
    wildfly_home: String,
    java_home: String,
    standalone_config: String,
    message: String,
    store: Option<RegistrationStore>,
    lifecycle: LifecycleController,
}

impl FlyDockApp {
    fn new() -> Self {
        let mut app = Self {
            wildfly_home: String::new(),
            java_home: String::new(),
            standalone_config: String::new(),
            message: String::new(),
            store: default_store_path().ok().map(RegistrationStore::new),
            lifecycle: LifecycleController::default(),
        };
        if let Some(store) = &app.store {
            match store.load() {
                Ok(Some(registration)) => app.set_registration(registration),
                Ok(None) => {}
                Err(error) => app.message = error.to_string(),
            }
        } else {
            app.message = "No se encontró una carpeta de datos de usuario".into();
        }
        app
    }

    fn set_registration(&mut self, registration: Registration) {
        self.wildfly_home = registration.wildfly_home;
        self.java_home = registration.java_home;
        self.standalone_config = registration.standalone_config;
    }

    fn choose_folder(current: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        if !current.is_empty() {
            dialog = dialog.set_directory(current);
        }
        dialog.pick_folder()
    }

    fn choose_config(current: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().add_filter("WildFly standalone.xml", &["xml"]);
        if !current.is_empty() {
            if let Some(parent) = std::path::Path::new(current).parent() {
                dialog = dialog.set_directory(parent);
            }
        }
        dialog.pick_file()
    }

    fn path_row(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut String,
        pick: impl FnOnce(&str) -> Option<PathBuf>,
    ) {
        ui.label(label);
        ui.horizontal(|ui| {
            ui.text_edit_singleline(value);
            if ui.button("Examinar…").clicked() {
                if let Some(path) = pick(value) {
                    *value = path.to_string_lossy().into_owned();
                }
            }
        });
    }
}

impl eframe::App for FlyDockApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("FlyDock");
            ui.label("Registro local de WildFly y JDK");
            ui.separator();
            Self::path_row(
                ui,
                "Carpeta de instalación WildFly:",
                &mut self.wildfly_home,
                Self::choose_folder,
            );
            Self::path_row(
                ui,
                "Carpeta del JDK:",
                &mut self.java_home,
                Self::choose_folder,
            );
            Self::path_row(
                ui,
                "Configuración standalone.xml:",
                &mut self.standalone_config,
                Self::choose_config,
            );
            ui.add_space(8.0);
            if ui.button("Validar y guardar registro").clicked() {
                let registration = Registration {
                    wildfly_home: self.wildfly_home.clone(),
                    java_home: self.java_home.clone(),
                    standalone_config: self.standalone_config.clone(),
                };
                self.message = match &self.store {
                    Some(store) => store
                        .save(&registration)
                        .map(|()| "Registro guardado.".into())
                        .unwrap_or_else(|error| error.to_string()),
                    None => "No se encontró una carpeta de datos de usuario".into(),
                };
            }
            ui.separator();
            let state = self.lifecycle.state();
            ui.horizontal(|ui| {
                ui.label(format!("Estado del servidor: {}", state.label()));
                let can_start = state == LifecycleState::Stopped;
                if ui
                    .add_enabled(can_start, egui::Button::new("Iniciar WildFly"))
                    .clicked()
                {
                    let registration = Registration {
                        wildfly_home: self.wildfly_home.clone(),
                        java_home: self.java_home.clone(),
                        standalone_config: self.standalone_config.clone(),
                    };
                    self.message = self
                        .lifecycle
                        .start(&registration)
                        .err()
                        .unwrap_or_else(|| "Solicitud de inicio enviada.".into());
                }
                ui.add_enabled(
                    false,
                    egui::Button::new("Detención graceful no disponible"),
                )
                .on_hover_text("FlyDock no puede verificar que el endpoint de administración pertenece al proceso iniciado.");
            });
            ui.label("Puerto de administración: resuelto desde standalone.xml");
            if let Some(error) = self.lifecycle.last_error() {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if !self.message.is_empty() {
                ui.add_space(6.0);
                ui.label(&self.message);
            }
            ui.separator();
            ui.label("Salida del servidor (últimos 64 KiB):");
            let mut logs = self.lifecycle.logs();
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut logs)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "FlyDock",
        eframe::NativeOptions::default(),
        Box::new(|_creation_context| Ok(Box::new(FlyDockApp::new()))),
    )
}

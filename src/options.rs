//! Opciones globales de LaguCAM, independientes de los proyectos.
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    version: u32,
    theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            theme: Theme::Dark,
        }
    }
}

pub struct Options {
    pub open: bool,
    path: Option<PathBuf>,
    settings: Settings,
    error: Option<String>,
}

impl Options {
    pub fn load(ctx: &egui::Context) -> Self {
        let (path, mut error) = match crate::portable::config_file("settings.json") {
            Ok(path) => (Some(path), None),
            Err(error) => (None, Some(error)),
        };
        let mut settings = Settings::default();
        if let Some(path) = &path {
            match fs::read(path) {
                Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                    Ok(saved) if saved.version == 1 => settings = saved,
                    Ok(_) => error = Some("Versión desconocida de config/settings.json".into()),
                    Err(e) => error = Some(format!("config/settings.json no es válido: {e}")),
                },
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => error = Some(format!("No se pueden leer las preferencias: {e}")),
            }
        }
        apply_theme(ctx, settings.theme);
        Self {
            open: false,
            path,
            settings,
            error,
        }
    }

    fn save(&mut self) {
        let result = (|| -> Result<(), String> {
            let path = self.path.as_ref().ok_or("Carpeta portable no disponible")?;
            let bytes = serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?;
            crate::portable::atomic_write(path, &bytes)
        })();
        self.error = result.err();
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Opciones de LaguCAM")
            .open(&mut open)
            .default_width(420.0)
            .min_width(340.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.heading("Apariencia");
                ui.separator();
                ui.label("Tema de la interfaz");
                let mut theme = self.settings.theme;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut theme, Theme::Dark, "Oscuro");
                    ui.selectable_value(&mut theme, Theme::Light, "Claro");
                });
                if theme != self.settings.theme {
                    self.settings.theme = theme;
                    apply_theme(ctx, theme);
                    self.save();
                }
                ui.add_space(8.0);
                ui.small("El cambio se aplica inmediatamente, sin reiniciar.");
                ui.small("El visor 3D conserva su fondo azul.");
                ui.separator();
                ui.label("Configuración portable");
                ui.monospace("config/settings.json");
                ui.small("Las preferencias se guardan junto al ejecutable, no en AppData.");
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error.as_str());
                }
            });
        self.open = open;
    }
}

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    let (panel, window, faint, extreme, inactive, hovered, active, accent) = match theme {
        Theme::Dark => (
            [30, 35, 43],
            [39, 45, 54],
            [47, 55, 66],
            [23, 28, 35],
            [53, 61, 72],
            [67, 80, 96],
            [57, 103, 150],
            [78, 149, 216],
        ),
        Theme::Light => (
            [231, 236, 242],
            [248, 250, 253],
            [218, 226, 236],
            [255, 255, 255],
            [222, 230, 239],
            [205, 219, 234],
            [159, 191, 225],
            [34, 103, 169],
        ),
    };
    visuals.panel_fill = egui::Color32::from_rgb(panel[0], panel[1], panel[2]);
    visuals.window_fill = egui::Color32::from_rgb(window[0], window[1], window[2]);
    visuals.faint_bg_color = egui::Color32::from_rgb(faint[0], faint[1], faint[2]);
    visuals.extreme_bg_color = egui::Color32::from_rgb(extreme[0], extreme[1], extreme[2]);
    visuals.widgets.inactive.bg_fill =
        egui::Color32::from_rgb(inactive[0], inactive[1], inactive[2]);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(hovered[0], hovered[1], hovered[2]);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(active[0], active[1], active[2]);
    visuals.selection.bg_fill = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
    visuals.hyperlink_color = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
    ctx.set_visuals(visuals);
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_dark() {
        assert_eq!(Settings::default().theme, Theme::Dark);
    }

    #[test]
    fn settings_file_roundtrips_both_themes() {
        for theme in [Theme::Dark, Theme::Light] {
            let settings = Settings { version: 1, theme };
            let json = serde_json::to_vec(&settings).unwrap();
            let restored: Settings = serde_json::from_slice(&json).unwrap();
            assert_eq!(restored.theme, theme);
            assert_eq!(restored.version, 1);
        }
    }
}

//! Paneles de trabajo independientes del proyecto y de los formatos geométricos.
//! La disposición es una preferencia del usuario, no parte de `.lagucam`.
use crate::{project::Project, project_ui};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Place {
    #[default]
    Left,
    Right,
    Floating,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Project,
    Properties,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Self::Project => "Proyecto",
            Self::Properties => "Propiedades",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct PaneState {
    place: Place,
    pos: [f32; 2],
    size: [f32; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    version: u32,
    project: PaneState,
    properties: PaneState,
    left_width: f32,
    right_width: f32,
    left_top_height: f32,
    right_top_height: f32,
    project_first: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            project: PaneState {
                place: Place::Left,
                pos: [95.0, 115.0],
                size: [325.0, 310.0],
            },
            properties: PaneState {
                place: Place::Left,
                pos: [430.0, 150.0],
                size: [360.0, 420.0],
            },
            left_width: 305.0,
            right_width: 305.0,
            left_top_height: 285.0,
            right_top_height: 285.0,
            project_first: true,
        }
    }
}

impl Settings {
    fn validate(&self) -> bool {
        self.version == 1
            && (200.0..=700.0).contains(&self.left_width)
            && (200.0..=700.0).contains(&self.right_width)
            && (140.0..=3000.0).contains(&self.left_top_height)
            && (140.0..=3000.0).contains(&self.right_top_height)
            && [self.project, self.properties].iter().all(|p| {
                p.pos
                    .iter()
                    .all(|v| v.is_finite() && (-20_000.0..=20_000.0).contains(v))
                    && (200.0..=1500.0).contains(&p.size[0])
                    && (140.0..=1500.0).contains(&p.size[1])
            })
    }
}

pub struct PanelLayout {
    settings: Settings,
    path: Option<PathBuf>,
    dirty: bool,
    window_generation: u32,
}

impl PanelLayout {
    pub fn load() -> Self {
        let path = layout_path();
        let settings = path
            .as_ref()
            .and_then(|p| fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .filter(Settings::validate)
            .unwrap_or_default();
        Self {
            settings,
            path,
            dirty: false,
            window_generation: 0,
        }
    }

    pub fn reset(&mut self) {
        self.settings = Settings::default();
        self.dirty = true;
        self.window_generation = self.window_generation.wrapping_add(1);
    }

    fn state(&self, pane: Pane) -> PaneState {
        match pane {
            Pane::Project => self.settings.project,
            Pane::Properties => self.settings.properties,
        }
    }

    fn set_state(&mut self, pane: Pane, state: PaneState) {
        match pane {
            Pane::Project => self.settings.project = state,
            Pane::Properties => self.settings.properties = state,
        }
        self.dirty = true;
    }

    fn move_to(&mut self, pane: Pane, place: Place, ctx: &egui::Context) {
        let mut state = self.state(pane);
        if state.place != place {
            state.place = place;
            self.set_state(pane, state);
            // Reutilizar las medidas flotantes guardadas, no las del contenido.
            self.window_generation = self.window_generation.wrapping_add(1);
            ctx.request_repaint();
        }
    }

    fn both_in(&self, place: Place) -> bool {
        self.settings.project.place == place && self.settings.properties.place == place
    }

    fn panes_for(&self, place: Place) -> Vec<Pane> {
        let mut panes = Vec::with_capacity(2);
        for pane in [Pane::Project, Pane::Properties] {
            if self.state(pane).place == place {
                panes.push(pane);
            }
        }
        if !self.settings.project_first {
            panes.reverse();
        }
        panes
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        pane: Pane,
        project: &mut Project,
        error: &mut String,
        enabled: bool,
        floating: bool,
    ) -> bool {
        let mut open_model = false;
        ui.horizontal(|ui| {
            if !floating {
                ui.strong(pane.title());
            }
            ui.menu_button("Posición", |ui| {
                for (place, title) in [
                    (Place::Left, "Acoplar a la izquierda"),
                    (Place::Right, "Acoplar a la derecha"),
                    (Place::Floating, "Desacoplar / Flotante"),
                ] {
                    if ui.button(title).clicked() {
                        self.move_to(pane, place, ui.ctx());
                        ui.close();
                    }
                }
                if !floating && self.both_in(self.state(pane).place) {
                    ui.separator();
                    if ui.button("Intercambiar arriba / abajo").clicked() {
                        self.settings.project_first = !self.settings.project_first;
                        self.dirty = true;
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(enabled, |ui| {
            // Área de desplazamiento que no cambia el tamaño de la ventana.
            egui::ScrollArea::vertical()
                .id_salt((pane.title(), "scroll"))
                .auto_shrink([false, false])
                .show(ui, |ui| match pane {
                    Pane::Project => project_ui::tree(ui, project),
                    Pane::Properties => {
                        open_model = project_ui::properties(ui, project, error);
                    }
                });
        });
        open_model
    }

    fn stack(
        &mut self,
        ui: &mut egui::Ui,
        place: Place,
        project: &mut Project,
        error: &mut String,
        enabled: bool,
    ) -> bool {
        let panes = self.panes_for(place);
        if panes.is_empty() {
            return false;
        }
        if panes.len() == 1 {
            return self.content(ui, panes[0], project, error, enabled, false);
        }
        let top_height = match place {
            Place::Left => self.settings.left_top_height,
            Place::Right => self.settings.right_top_height,
            Place::Floating => return false,
        };
        let top = panes[0];
        let bottom = panes[1];
        let top_response =
            egui::Panel::top(("lagucam_panel_top", place as u8, self.window_generation))
                .resizable(true)
                .default_size(top_height)
                .size_range(140.0..=3000.0)
                .show(ui, |ui| {
                    self.content(ui, top, project, error, enabled, false)
                });
        let top_value = top_response.response.rect.height();
        if top_value.is_finite() && (140.0..=3000.0).contains(&top_value) {
            match place {
                Place::Left => {
                    if (top_value - self.settings.left_top_height).abs() > 0.5 {
                        self.settings.left_top_height = top_value;
                        self.dirty = true;
                    }
                }
                Place::Right => {
                    if (top_value - self.settings.right_top_height).abs() > 0.5 {
                        self.settings.right_top_height = top_value;
                        self.dirty = true;
                    }
                }
                Place::Floating => {}
            }
        }
        let bottom_response = egui::CentralPanel::default().show(ui, |ui| {
            self.content(ui, bottom, project, error, enabled, false)
        });
        top_response.inner || bottom_response.inner
    }

    pub fn show_docked(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        error: &mut String,
        enabled: bool,
    ) -> bool {
        let mut open_model = false;
        if !self.panes_for(Place::Left).is_empty() {
            let r = egui::Panel::left(("lagucam_dock_left", self.window_generation))
                .resizable(true)
                .default_size(self.settings.left_width)
                .size_range(220.0..=700.0)
                .show(ui, |ui| {
                    self.stack(ui, Place::Left, project, error, enabled)
                });
            if (r.response.rect.width() - self.settings.left_width).abs() > 0.5 {
                self.settings.left_width = r.response.rect.width();
                self.dirty = true;
            }
            open_model |= r.inner;
        }
        if !self.panes_for(Place::Right).is_empty() {
            let r = egui::Panel::right(("lagucam_dock_right", self.window_generation))
                .resizable(true)
                .default_size(self.settings.right_width)
                .size_range(220.0..=700.0)
                .show(ui, |ui| {
                    self.stack(ui, Place::Right, project, error, enabled)
                });
            if (r.response.rect.width() - self.settings.right_width).abs() > 0.5 {
                self.settings.right_width = r.response.rect.width();
                self.dirty = true;
            }
            open_model |= r.inner;
        }
        open_model
    }

    pub fn show_floating(
        &mut self,
        ctx: &egui::Context,
        project: &mut Project,
        error: &mut String,
        enabled: bool,
    ) -> bool {
        let mut open_model = false;
        for pane in [Pane::Project, Pane::Properties] {
            let state = self.state(pane);
            if state.place != Place::Floating {
                continue;
            }
            let window_id =
                egui::Id::unique(("lagucam_window", pane.title(), self.window_generation));
            let response = egui::Window::new(pane.title())
                .id(window_id)
                .default_pos(egui::pos2(state.pos[0], state.pos[1]))
                .default_size(egui::vec2(state.size[0], state.size[1]))
                .min_size(egui::vec2(220.0, 145.0))
                .resizable(true)
                .collapsible(false)
                .scroll(false)
                .show(ctx, |ui| {
                    self.content(ui, pane, project, error, enabled, true)
                });
            if let Some(response) = response {
                open_model |= response.inner.unwrap_or(false);
                let rect = response.response.rect;
                // Actualizar la geometría solo si se sigue en modo flotante.
                if self.state(pane).place == Place::Floating {
                    let new_pos = [rect.min.x, rect.min.y];
                    let new_size = [rect.width(), rect.height()];
                    if new_pos.iter().chain(new_size.iter()).all(|v| v.is_finite())
                        && new_size[0] >= 200.0
                        && new_size[1] >= 140.0
                    {
                        let current = self.state(pane);
                        if current.pos != new_pos || current.size != new_size {
                            self.set_state(
                                pane,
                                PaneState {
                                    pos: new_pos,
                                    size: new_size,
                                    ..current
                                },
                            );
                        }
                    }
                    // Arrastrar hasta el borde izquierdo/derecho para pegar la ventana.
                    if ctx.input(|i| i.pointer.any_released())
                        && let Some(side) = snap_target(rect, ctx.content_rect())
                    {
                        self.move_to(pane, side, ctx);
                    }
                }
            }
        }
        open_model
    }

    pub fn save_if_needed(&mut self, ctx: &egui::Context) {
        if self.dirty && ctx.input(|i| i.pointer.any_released()) {
            self.save();
        }
    }

    pub fn save(&mut self) {
        if let Some(path) = &self.path {
            let result = (|| -> Result<(), String> {
                let parent = path.parent().ok_or("Ruta de configuración inválida")?;
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let json = serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?;
                // Solo configuración visual, nunca la biblioteca ni el proyecto.
                fs::write(path, json).map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(e) = result {
                eprintln!("No se pudo guardar la distribución de paneles: {e}");
                return;
            }
            self.dirty = false;
        }
    }
}

fn snap_target(window: egui::Rect, screen: egui::Rect) -> Option<Place> {
    if window.left() <= screen.left() + 22.0 {
        Some(Place::Left)
    } else if window.right() >= screen.right() - 22.0 {
        Some(Place::Right)
    } else {
        None
    }
}

fn layout_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("LaguCAM").join("panels.json"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join("Library/Application Support/LaguCAM/panels.json"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("lagucam").join("panels.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_stacked_at_left() {
        let layout = PanelLayout::load();
        let mut fresh = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
        };
        assert_eq!(
            fresh.panes_for(Place::Left),
            vec![Pane::Project, Pane::Properties]
        );
        fresh.settings.project_first = false;
        assert_eq!(
            fresh.panes_for(Place::Left),
            vec![Pane::Properties, Pane::Project]
        );
        assert!(layout.settings.validate());
    }

    #[test]
    fn user_layout_has_strict_version_and_finite_dimensions() {
        let mut settings = Settings::default();
        assert!(settings.validate());
        settings.version = 77;
        assert!(!settings.validate());
        settings = Settings::default();
        settings.project.size[1] = f32::NAN;
        assert!(!settings.validate());
    }

    #[test]
    fn floating_panels_snap_only_at_screen_edges() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let middle = egui::Rect::from_min_size(egui::pos2(200.0, 120.0), egui::vec2(300.0, 300.0));
        let left = middle.translate(egui::vec2(-195.0, 0.0));
        let right = middle.translate(egui::vec2(690.0, 0.0));
        assert_eq!(snap_target(middle, screen), None);
        assert_eq!(snap_target(left, screen), Some(Place::Left));
        assert_eq!(snap_target(right, screen), Some(Place::Right));
    }

    #[test]
    fn location_switch_does_not_change_project_data() {
        let mut layout = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
        };
        let ctx = egui::Context::default();
        layout.move_to(Pane::Properties, Place::Floating, &ctx);
        assert_eq!(layout.state(Pane::Properties).place, Place::Floating);
        layout.move_to(Pane::Properties, Place::Right, &ctx);
        assert_eq!(layout.state(Pane::Properties).place, Place::Right);
        layout.reset();
        assert!(layout.both_in(Place::Left));
    }

    #[test]
    fn settings_json_roundtrip() {
        let settings = Settings::default();
        let bytes = serde_json::to_vec(&settings).unwrap();
        let decoded: Settings = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.validate());
        assert_eq!(decoded.project.place, Place::Left);
        assert_eq!(decoded.properties.place, Place::Left);
    }
}

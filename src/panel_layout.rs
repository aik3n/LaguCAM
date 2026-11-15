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

    fn other(self) -> Self {
        match self {
            Self::Project => Self::Properties,
            Self::Properties => Self::Project,
        }
    }
}

/// Destino de una suelta de panel: borde lateral y mitad superior/inferior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DockTarget {
    side: Place,
    slot: Slot,
}

#[derive(Clone, Copy, Debug)]
struct DockDrag {
    pane: Pane,
    start: egui::Pos2,
    // En ventanas flotantes, una pulsación sin movimiento NO acopla.
    active: bool,
}

/// Las zonas se definen por la posición del CURSOR, no por el marco de la ventana:
/// evita acoplar accidentalmente al redimensionar o al soltar dentro del contenido.
/// Asa de seis puntos dibujada por egui: no depende de glifos ni fuentes.
fn panel_drag_handle(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 23.0), egui::Sense::drag());
    let highlighted = response.hovered() || response.dragged();
    if highlighted {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let ink = if highlighted {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.noninteractive.fg_stroke.color
    };
    for dx in [-4.0_f32, 4.0] {
        for dy in [-5.0_f32, 0.0, 5.0] {
            ui.painter()
                .circle_filled(rect.center() + egui::vec2(dx, dy), 1.5, ink);
        }
    }
    response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text("Arrastrar panel")
}

/// La representación flotante sigue al puntero sin salirse de la pantalla.
fn ghost_bounds(pointer: egui::Pos2, screen: egui::Rect, size: egui::Vec2) -> egui::Rect {
    let min_x = screen.left() + 6.0;
    let min_y = screen.top() + 6.0;
    let max_x = (screen.right() - size.x - 6.0).max(min_x);
    let max_y = (screen.bottom() - size.y - 6.0).max(min_y);
    let pos = egui::pos2(
        (pointer.x + 22.0).clamp(min_x, max_x),
        (pointer.y + 16.0).clamp(min_y, max_y),
    );
    egui::Rect::from_min_size(pos, size)
}
fn target_at(pointer: egui::Pos2, screen: egui::Rect) -> Option<DockTarget> {
    if !screen.contains(pointer) || screen.width() < 350.0 || screen.height() < 280.0 {
        return None;
    }
    let band = (screen.width() * 0.19).min(115.0);
    let side = if pointer.x <= screen.left() + band {
        Place::Left
    } else if pointer.x >= screen.right() - band {
        Place::Right
    } else {
        return None;
    };
    let slot = if pointer.y < screen.center().y {
        Slot::Top
    } else {
        Slot::Bottom
    };
    Some(DockTarget { side, slot })
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
    dragging: Option<DockDrag>,
    edit_operation: Option<usize>,
}

impl PanelLayout {
    pub fn load() -> Self {
        let path = crate::portable::config_file("panels.json").ok();
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
            dragging: None,
            edit_operation: None,
        }
    }

    pub fn take_edit_operation(&mut self) -> Option<usize> {
        self.edit_operation.take()
    }

    pub fn reset(&mut self) {
        self.settings = Settings::default();
        self.dirty = true;
        self.window_generation = self.window_generation.wrapping_add(1);
        self.dragging = None;
        self.edit_operation = None;
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
        if !floating {
            ui.horizontal(|ui| {
                let handle = panel_drag_handle(ui);
                if handle.drag_started_by(egui::PointerButton::Primary) {
                    let start = ui
                        .ctx()
                        .input(|i| i.pointer.interact_pos())
                        .unwrap_or(handle.rect.center());
                    self.dragging = Some(DockDrag {
                        pane,
                        start,
                        active: false,
                    });
                    ui.ctx().request_repaint();
                }
                ui.add_space(2.0);
                ui.strong(pane.title());
            });
            ui.separator();
        }
        // En modo flotante, la cabecera nativa de egui ya permite arrastrar.
        ui.add_enabled_ui(enabled, |ui| {
            egui::ScrollArea::vertical()
                .id_salt((pane.title(), "scroll"))
                .auto_shrink([false, false])
                .show(ui, |ui| match pane {
                    Pane::Project => {
                        let actions = project_ui::tree(ui, project);
                        if let Some(index) = actions.edit {
                            self.edit_operation = Some(index);
                        }
                    }
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
                    // Solo arrastrar la BARRA DE TÍTULO nativa inicia docking.
                    // Redimensionar, editar o soltar sobre el contenido no cambia de zona.
                    if self.dragging.is_none()
                        && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                        && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
                        && rect.contains(pos)
                        && pos.y <= rect.top() + 29.0
                    {
                        self.dragging = Some(DockDrag {
                            pane,
                            start: pos,
                            active: false,
                        });
                    }
                }
            }
        }
        self.drag_preview_and_drop(ctx);
        open_model
    }

    /// Vista previa visual sin etiquetas; no intercepta los eventos del ratón.
    fn drag_preview_and_drop(&mut self, ctx: &egui::Context) {
        let Some(mut drag) = self.dragging else {
            return;
        };
        let (pointer, released, down) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Primary),
            )
        });
        let Some(pointer) = pointer else {
            if released || !down {
                self.dragging = None;
            }
            return;
        };
        if (pointer - drag.start).length_sq() > 64.0 {
            drag.active = true;
        }
        let screen = ctx.content_rect();
        let target = if drag.active {
            target_at(pointer, screen)
        } else {
            None
        };
        if released || !down {
            self.dragging = None;
            if drag.active {
                if let Some(target) = target {
                    self.dock_drop(drag.pane, target, ctx);
                } else if self.state(drag.pane).place != Place::Floating {
                    self.float_drop(drag.pane, pointer, screen, ctx);
                }
            }
            return;
        }
        self.dragging = Some(drag);
        if !drag.active {
            return;
        }
        ctx.request_repaint();
        let layer = egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("lagucam_docking_guides"),
        );
        let painter = ctx.layer_painter(layer);
        let dark = ctx.theme() == egui::Theme::Dark;
        let (guide_bg, guide_border, interior, accent, ghost_bg, ghost_header, ghost_text) = if dark
        {
            (
                egui::Color32::from_rgba_unmultiplied(31, 43, 57, 224),
                egui::Color32::from_rgb(104, 135, 163),
                egui::Color32::from_rgb(64, 82, 102),
                egui::Color32::from_rgb(86, 173, 240),
                egui::Color32::from_rgba_unmultiplied(31, 40, 52, 205),
                egui::Color32::from_rgba_unmultiplied(58, 76, 96, 224),
                egui::Color32::from_rgb(232, 242, 250),
            )
        } else {
            (
                egui::Color32::from_rgba_unmultiplied(247, 250, 253, 238),
                egui::Color32::from_rgb(127, 154, 177),
                egui::Color32::from_rgb(219, 229, 238),
                egui::Color32::from_rgb(37, 115, 187),
                egui::Color32::from_rgba_unmultiplied(240, 246, 252, 224),
                egui::Color32::from_rgba_unmultiplied(211, 226, 240, 240),
                egui::Color32::from_rgb(33, 56, 79),
            )
        };
        // La zona final ocupa el espacio real que tendrá el panel.
        if let Some(target) = target {
            let rect = self.preview_rect(drag.pane, target, screen);
            painter.rect_filled(
                rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(72, 161, 228, 52),
            );
            painter.rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(2.0, accent),
                egui::StrokeKind::Inside,
            );
        }
        // Cuatro pictogramas geométricos: izquierda/derecha, superior/inferior.
        for (side, x) in [
            (Place::Left, screen.left() + 39.0),
            (Place::Right, screen.right() - 39.0),
        ] {
            for (slot, dy) in [(Slot::Top, -29.0), (Slot::Bottom, 29.0)] {
                let box_rect = egui::Rect::from_center_size(
                    egui::pos2(x, screen.center().y + dy),
                    egui::vec2(62.0, 46.0),
                );
                let selected = target == Some(DockTarget { side, slot });
                painter.rect_filled(box_rect, 5.0, if selected { accent } else { guide_bg });
                painter.rect_stroke(
                    box_rect,
                    5.0,
                    egui::Stroke::new(if selected { 2.0 } else { 1.0 }, guide_border),
                    egui::StrokeKind::Inside,
                );
                let outline = box_rect.shrink2(egui::vec2(13.0, 9.0));
                painter.rect_filled(outline, 2.0, interior);
                let half = match slot {
                    Slot::Top => egui::Rect::from_min_max(
                        outline.min,
                        egui::pos2(outline.right(), outline.center().y),
                    ),
                    Slot::Bottom => egui::Rect::from_min_max(
                        egui::pos2(outline.left(), outline.center().y),
                        outline.max,
                    ),
                };
                painter.rect_filled(
                    half.shrink(1.0),
                    1.0,
                    if selected { guide_bg } else { accent },
                );
                painter.rect_stroke(
                    outline,
                    2.0,
                    egui::Stroke::new(1.0, guide_border),
                    egui::StrokeKind::Inside,
                );
            }
        }
        // Un panel acoplado no tiene ventana real: mostrar su silueta flotante.
        // Si ya es flotante, la ventana nativa de egui sigue el ratón.
        if self.state(drag.pane).place != Place::Floating {
            let pane_size = self.state(drag.pane).size;
            let size = egui::vec2(pane_size[0].clamp(245.0, 335.0), 155.0);
            let ghost = ghost_bounds(pointer, screen, size);
            let header = egui::Rect::from_min_size(ghost.min, egui::vec2(ghost.width(), 34.0));
            painter.rect_filled(ghost, 6.0, ghost_bg);
            painter.rect_filled(header, 6.0, ghost_header);
            painter.rect_stroke(
                ghost,
                6.0,
                egui::Stroke::new(1.25, accent),
                egui::StrokeKind::Inside,
            );
            painter.text(
                header.left_center() + egui::vec2(16.0, 0.0),
                egui::Align2::LEFT_CENTER,
                drag.pane.title(),
                egui::FontId::proportional(14.0),
                ghost_text,
            );
            // Trazos abstractos: insinúan el contenido sin duplicar controles.
            for i in 0..3 {
                let bar = egui::Rect::from_min_size(
                    egui::pos2(
                        ghost.left() + 17.0,
                        header.bottom() + 17.0 + i as f32 * 26.0,
                    ),
                    egui::vec2(ghost.width() * (0.57 - i as f32 * 0.08), 7.0),
                );
                painter.rect_filled(bar, 3.0, guide_border);
            }
        }
    }

    fn preview_rect(&self, pane: Pane, target: DockTarget, screen: egui::Rect) -> egui::Rect {
        let width = match target.side {
            Place::Left => self.settings.left_width,
            Place::Right => self.settings.right_width,
            Place::Floating => 305.0,
        }
        .min((screen.width() - 160.0).max(220.0));
        let side = match target.side {
            Place::Left => egui::Rect::from_min_max(
                screen.min,
                egui::pos2(screen.left() + width, screen.bottom()),
            ),
            Place::Right => egui::Rect::from_min_max(
                egui::pos2(screen.right() - width, screen.top()),
                screen.max,
            ),
            Place::Floating => screen,
        };
        if self.state(pane.other()).place != target.side {
            return side;
        }
        let height = match target.side {
            Place::Left => self.settings.left_top_height,
            Place::Right => self.settings.right_top_height,
            Place::Floating => side.height() * 0.5,
        }
        .clamp(140.0, (side.height() - 140.0).max(140.0));
        match target.slot {
            Slot::Top => {
                egui::Rect::from_min_max(side.min, egui::pos2(side.right(), side.top() + height))
            }
            Slot::Bottom => {
                egui::Rect::from_min_max(egui::pos2(side.left(), side.top() + height), side.max)
            }
        }
    }

    fn dock_drop(&mut self, pane: Pane, target: DockTarget, ctx: &egui::Context) {
        // Si el otro panel está en el destino, la mitad indicada decide el orden.
        if self.state(pane.other()).place == target.side {
            self.settings.project_first = (pane == Pane::Project) == (target.slot == Slot::Top);
            self.dirty = true;
        }
        self.move_to(pane, target.side, ctx);
        ctx.request_repaint();
    }

    fn float_drop(
        &mut self,
        pane: Pane,
        pointer: egui::Pos2,
        screen: egui::Rect,
        ctx: &egui::Context,
    ) {
        let mut state = self.state(pane);
        state.pos = [
            (pointer.x - 110.0).clamp(screen.left(), (screen.right() - 220.0).max(screen.left())),
            (pointer.y - 18.0).clamp(screen.top(), (screen.bottom() - 145.0).max(screen.top())),
        ];
        self.set_state(pane, state);
        self.move_to(pane, Place::Floating, ctx);
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
                crate::portable::atomic_write(path, &json)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_ghost_remains_inside_normal_viewport() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let size = egui::vec2(335.0, 155.0);
        for pointer in [egui::pos2(0.0, 0.0), egui::pos2(1279.0, 799.0)] {
            let rect = ghost_bounds(pointer, screen, size);
            assert!(screen.contains(rect.min));
            assert!(screen.contains(rect.max));
            assert_eq!(rect.size(), size);
        }
    }
    #[test]
    fn default_is_stacked_at_left() {
        let layout = PanelLayout::load();
        let mut fresh = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
            dragging: None,
            edit_operation: None,
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
    fn docking_targets_follow_pointer_not_window_edges() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
        assert_eq!(target_at(egui::pos2(600.0, 200.0), screen), None);
        assert_eq!(
            target_at(egui::pos2(10.0, 100.0), screen),
            Some(DockTarget {
                side: Place::Left,
                slot: Slot::Top,
            })
        );
        assert_eq!(
            target_at(egui::pos2(1190.0, 790.0), screen),
            Some(DockTarget {
                side: Place::Right,
                slot: Slot::Bottom,
            })
        );
        assert_eq!(target_at(egui::pos2(-20.0, 50.0), screen), None);
    }

    #[test]
    fn dropping_onto_a_stacked_side_sets_order_independently() {
        let mut layout = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
            dragging: None,
            edit_operation: None,
        };
        let ctx = egui::Context::default();
        layout.dock_drop(
            Pane::Properties,
            DockTarget {
                side: Place::Left,
                slot: Slot::Top,
            },
            &ctx,
        );
        assert_eq!(
            layout.panes_for(Place::Left),
            vec![Pane::Properties, Pane::Project]
        );
        layout.dock_drop(
            Pane::Project,
            DockTarget {
                side: Place::Left,
                slot: Slot::Top,
            },
            &ctx,
        );
        assert_eq!(
            layout.panes_for(Place::Left),
            vec![Pane::Project, Pane::Properties]
        );
    }

    #[test]
    fn float_drop_keeps_saved_size_and_moves_only_target_pane() {
        let mut layout = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
            dragging: None,
            edit_operation: None,
        };
        let size_before = layout.state(Pane::Properties).size;
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
        layout.float_drop(Pane::Properties, egui::pos2(600.0, 360.0), screen, &ctx);
        assert_eq!(layout.state(Pane::Properties).place, Place::Floating);
        assert_eq!(layout.state(Pane::Project).place, Place::Left);
        assert_eq!(layout.state(Pane::Properties).size, size_before);
    }

    #[test]
    fn location_switch_does_not_change_project_data() {
        let mut layout = PanelLayout {
            settings: Settings::default(),
            path: None,
            dirty: false,
            window_generation: 0,
            dragging: None,
            edit_operation: None,
        };
        let ctx = egui::Context::default();
        layout.move_to(Pane::Properties, Place::Floating, &ctx);
        assert_eq!(layout.state(Pane::Properties).place, Place::Floating);
        layout.move_to(Pane::Properties, Place::Right, &ctx);
        assert_eq!(layout.state(Pane::Properties).place, Place::Right);
        layout.reset();
        assert_eq!(layout.state(Pane::Project).place, Place::Left);
        assert_eq!(layout.state(Pane::Properties).place, Place::Left);
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

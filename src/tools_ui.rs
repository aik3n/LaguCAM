//! Biblioteca global: listado, editor transaccional y dibujo técnico con egui.
//! Ninguna operación de esta interfaz altera el proyecto abierto ni recursos GL.
use crate::tools::{
    model::{Tool, ToolGeometry, ToolKind, ToolLibrary, ToolQuery, ToolSort},
    storage,
};
use eframe::egui;
use std::path::Path;

#[derive(Default)]
pub struct ToolUi {
    pub open: bool,
    query: ToolQuery,
    selected_id: Option<String>,
    draft: Option<Tool>,
    error: String,
    blocked: bool,
    confirm_delete: bool,
}

impl ToolUi {
    pub fn new(load_error: Option<String>) -> Self {
        Self {
            error: load_error.clone().unwrap_or_default(),
            blocked: load_error.is_some(),
            ..Default::default()
        }
    }

    fn has_pending_edit(&self, lib: &ToolLibrary) -> bool {
        match &self.draft {
            Some(draft) if draft.id.is_empty() => true,
            Some(draft) => lib.get(&draft.id).is_some_and(|original| original != draft),
            None => false,
        }
    }

    fn switch_to(&mut self, lib: &ToolLibrary, id: &str) {
        if self.has_pending_edit(lib) {
            self.error =
                "Guarda o cancela la edición actual antes de seleccionar otra herramienta".into();
            return;
        }
        self.selected_id = Some(id.to_owned());
        self.draft = lib.get(id).cloned();
        self.error.clear();
        self.confirm_delete = false;
    }

    fn begin_new(&mut self, lib: &ToolLibrary) {
        if self.has_pending_edit(lib) {
            self.error =
                "Guarda o cancela la edición actual antes de crear otra herramienta".into();
            return;
        }
        self.selected_id = None;
        self.draft = Some(Tool::blank(ToolKind::Flat));
        self.error.clear();
        self.confirm_delete = false;
    }

    fn duplicate(&mut self, lib: &ToolLibrary) {
        if self.has_pending_edit(lib) {
            self.error = "Guarda o cancela la edición actual antes de duplicar".into();
            return;
        }
        if let Some(id) = &self.selected_id {
            match lib.duplicate(id) {
                Ok(tool) => {
                    self.draft = Some(tool);
                    self.selected_id = None;
                    self.error.clear();
                }
                Err(err) => self.error = err,
            }
        }
        self.confirm_delete = false;
    }

    fn commit(&mut self, lib: &mut ToolLibrary, path: Option<&Path>) {
        let Some(path) = path else {
            self.error = "No hay directorio de configuración; no es posible guardar".into();
            return;
        };
        let Some(draft) = &self.draft else {
            return;
        };
        let mut candidate = lib.clone();
        let result = if draft.id.is_empty() {
            candidate.add(draft.clone())
        } else {
            candidate.replace(draft.clone()).map(|_| draft.id.clone())
        };
        match result.and_then(|id| storage::save(path, &candidate).map(|_| id)) {
            Ok(id) => {
                *lib = candidate;
                self.selected_id = Some(id.clone());
                self.draft = lib.get(&id).cloned();
                self.error.clear();
            }
            Err(err) => self.error = err,
        }
    }

    fn delete(&mut self, lib: &mut ToolLibrary, path: Option<&Path>) {
        let Some(path) = path else {
            self.error = "No hay directorio de configuración; no se puede eliminar".into();
            return;
        };
        let Some(id) = &self.selected_id else {
            return;
        };
        let mut candidate = lib.clone();
        match candidate
            .delete(id)
            .and_then(|_| storage::save(path, &candidate))
        {
            Ok(()) => {
                *lib = candidate;
                self.selected_id = None;
                self.draft = None;
                self.error.clear();
                self.confirm_delete = false;
            }
            Err(err) => self.error = err,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, lib: &mut ToolLibrary, path: Option<&Path>) {
        if !self.open {
            return;
        }
        let mut window_open = self.open;
        egui::Window::new("Biblioteca global de herramientas")
            .open(&mut window_open)
            .default_size([1130.0, 650.0])
            .min_width(900.0)
            .resizable(true)
            .show(ctx, |ui| {
                if self.blocked {
                    ui.colored_label(
                        egui::Color32::RED,
                        "La biblioteca guardada no se puede leer. No se sobrescribirá.",
                    );
                    ui.small("Revisa o respalda tools.json antes de recuperar la biblioteca.");
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
                ui.separator();
                ui.columns(3, |columns| {
                    columns[0].heading("Listado y filtros");
                    columns[1].heading("Propiedades");
                    columns[2].heading("Perfil técnico");
                    columns[0].separator();
                    columns[1].separator();
                    columns[2].separator();
                    self.list_panel(&mut columns[0], lib, path);
                    self.editor_panel(&mut columns[1], lib, path);
                    self.profile_panel(&mut columns[2], lib);
                });
                ui.separator();
                ui.small("Biblioteca de usuario independiente de proyectos y máquinas · sin T fijo · dimensiones en mm.");
            });
        self.open = window_open;
    }

    fn list_panel(&mut self, ui: &mut egui::Ui, lib: &mut ToolLibrary, path: Option<&Path>) {
        ui.horizontal(|ui| {
            ui.label("Nombre");
            ui.text_edit_singleline(&mut self.query.search);
        });
        egui::ComboBox::from_label("Tipo")
            .selected_text(self.query.kind.map_or("Todos", ToolKind::label))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.query.kind, None, "Todos");
                for kind in ToolKind::ALL {
                    ui.selectable_value(&mut self.query.kind, Some(kind), kind.label());
                }
            });
        ui.label("Diámetro de corte [mm]");
        ui.horizontal(|ui| {
            ui.label("Mín.");
            ui.add(
                egui::DragValue::new(&mut self.query.min_diameter_mm)
                    .range(0.0..=1.0e100)
                    .speed(0.5),
            );
            let mut maximum_enabled = self.query.max_diameter_mm.is_some();
            if ui.checkbox(&mut maximum_enabled, "Máx.").changed() {
                self.query.max_diameter_mm = maximum_enabled.then_some(10.0);
            }
            if let Some(maximum) = &mut self.query.max_diameter_mm {
                ui.add(
                    egui::DragValue::new(maximum)
                        .range(0.0..=f64::MAX)
                        .speed(0.5),
                );
            }
        });
        egui::ComboBox::from_label("Orden")
            .selected_text(self.query.sort.label())
            .show_ui(ui, |ui| {
                for sort in ToolSort::ALL {
                    ui.selectable_value(&mut self.query.sort, sort, sort.label());
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.blocked, egui::Button::new("Nueva"))
                .clicked()
            {
                self.begin_new(lib);
            }
            if ui
                .add_enabled(
                    !self.blocked && self.selected_id.is_some(),
                    egui::Button::new("Duplicar"),
                )
                .clicked()
            {
                self.duplicate(lib);
            }
        });
        let list: Vec<_> = self
            .query
            .filtered(lib)
            .into_iter()
            .map(|t| {
                (
                    t.id.clone(),
                    t.name.clone(),
                    t.geometry.kind(),
                    t.geometry.diameter_mm(),
                )
            })
            .collect();
        ui.label(format!(
            "{} herramientas de {}",
            list.len(),
            lib.tools.len()
        ));
        egui::ScrollArea::vertical()
            .max_height(390.0)
            .show(ui, |ui| {
                for (id, name, kind, diameter) in list {
                    let selected = self.selected_id.as_deref() == Some(id.as_str());
                    if ui
                        .selectable_label(
                            selected,
                            format!("{} · Ø{diameter:.3} · {name}", kind.label()),
                        )
                        .clicked()
                    {
                        self.switch_to(lib, &id);
                    }
                }
            });
        ui.separator();
        if self.selected_id.is_some() && !self.blocked {
            if !self.confirm_delete {
                if ui.button("Eliminar…").clicked() {
                    self.confirm_delete = true;
                }
            } else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "¿Eliminar definitivamente esta herramienta?",
                );
                ui.horizontal(|ui| {
                    if ui.button("Confirmar borrado").clicked() {
                        self.delete(lib, path);
                    }
                    if ui.button("Conservar").clicked() {
                        self.confirm_delete = false;
                    }
                });
            }
        }
    }

    fn editor_panel(&mut self, ui: &mut egui::Ui, lib: &mut ToolLibrary, path: Option<&Path>) {
        let mut save = false;
        let mut cancel = false;
        let Some(draft) = self.draft.as_mut() else {
            ui.label("Selecciona una herramienta o pulsa Nueva.");
            return;
        };
        let is_new = draft.id.is_empty();
        ui.small(if is_new {
            "Nueva definición (sin guardar)"
        } else {
            "Edición pendiente de confirmar"
        });
        if !is_new {
            ui.small(format!("ID: {} · Revisión: {}", draft.id, draft.revision));
        }
        egui::ScrollArea::vertical()
            .max_height(475.0)
            .show(ui, |ui| {
                ui.label("Nombre");
                ui.text_edit_singleline(&mut draft.name);
                let mut kind = draft.geometry.kind();
                egui::ComboBox::from_label("Geometría")
                    .selected_text(kind.label())
                    .show_ui(ui, |ui| {
                        for option in ToolKind::ALL {
                            ui.selectable_value(&mut kind, option, option.label());
                        }
                    });
                if kind != draft.geometry.kind() {
                    draft.geometry = ToolGeometry::blank(kind);
                }
                ui.separator();
                ui.label("Geometría de corte");
                geometry_fields(ui, &mut draft.geometry);
                number(ui, "Longitud de corte [mm]", &mut draft.cutting_length_mm);
                ui.separator();
                ui.label("Parte no cortante y longitud total");
                number(ui, "Diámetro del mango [mm]", &mut draft.shank_diameter_mm);
                number(ui, "Longitud total [mm]", &mut draft.total_length_mm);
                ui.small("Desde la punta hasta el extremo del mango; no es la salida de la pinza.");
                ui.separator();
                ui.label("Referencia comercial (opcional)");
                ui.text_edit_singleline(&mut draft.commercial_reference);
                ui.label("Descripción (opcional)");
                ui.text_edit_multiline(&mut draft.description);
                ui.separator();
                ui.label("Tecnología recomendada (opcional, no validada para máquinas)");
                optional_u32(ui, "Número de filos", &mut draft.flute_count);
                optional_u32(ui, "RPM recomendadas", &mut draft.recommended_rpm);
                optional_f64(
                    ui,
                    "Avance recomendado [mm/min]",
                    &mut draft.recommended_feed_mm_min,
                );
                ui.label("Material recomendado");
                ui.text_edit_singleline(&mut draft.recommended_material);
                ui.label("Notas");
                ui.text_edit_multiline(&mut draft.notes);
            });
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.blocked && path.is_some(),
                    egui::Button::new("Guardar herramienta"),
                )
                .clicked()
            {
                save = true;
            }
            if ui.button("Cancelar edición").clicked() {
                cancel = true;
            }
        });
        if save {
            self.commit(lib, path);
        }
        if cancel {
            self.draft = self
                .selected_id
                .as_ref()
                .and_then(|id| lib.get(id))
                .cloned();
            self.confirm_delete = false;
            self.error.clear();
        }
    }

    fn profile_panel(&self, ui: &mut egui::Ui, lib: &ToolLibrary) {
        let tool = self
            .draft
            .as_ref()
            .or_else(|| self.selected_id.as_ref().and_then(|id| lib.get(id)));
        if let Some(tool) = tool {
            crate::tools_profile::draw(ui, tool);
        } else {
            crate::tools_profile::draw_empty(ui);
        }
    }
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.1).max_decimals(9));
    });
}

fn geometry_fields(ui: &mut egui::Ui, geometry: &mut ToolGeometry) {
    match geometry {
        ToolGeometry::Flat {
            cutting_diameter_mm,
        }
        | ToolGeometry::BallNose {
            cutting_diameter_mm,
        } => {
            number(ui, "Diámetro corte Ø [mm]", cutting_diameter_mm);
        }
        ToolGeometry::VBit {
            cutting_diameter_mm,
            included_angle_deg,
        } => {
            number(ui, "Diámetro máximo corte Ø [mm]", cutting_diameter_mm);
            number(ui, "Ángulo total [°]", included_angle_deg);
        }
        ToolGeometry::ConicalFlat {
            cutting_diameter_mm,
            tip_diameter_mm,
            included_angle_deg,
        } => {
            number(ui, "Diámetro máximo corte Ø [mm]", cutting_diameter_mm);
            number(ui, "Diámetro de punta plana Ø [mm]", tip_diameter_mm);
            number(ui, "Ángulo total [°]", included_angle_deg);
        }
        ToolGeometry::ConicalBall {
            cutting_diameter_mm,
            tip_sphere_radius_mm,
            included_angle_deg,
        } => {
            number(ui, "Diámetro máximo corte Ø [mm]", cutting_diameter_mm);
            number(ui, "Radio esférico de punta R [mm]", tip_sphere_radius_mm);
            number(ui, "Ángulo total [°]", included_angle_deg);
        }
    }
}

fn optional_u32(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) {
    let mut enabled = value.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, label).changed() {
            *value = enabled.then_some(0);
        }
        if let Some(number) = value {
            ui.add(egui::DragValue::new(number).range(0..=u32::MAX).speed(1.0));
        }
    });
}

fn optional_f64(ui: &mut egui::Ui, label: &str, value: &mut Option<f64>) {
    let mut enabled = value.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, label).changed() {
            *value = enabled.then_some(0.0);
        }
        if let Some(number) = value {
            ui.add(egui::DragValue::new(number).speed(1.0));
        }
    });
}

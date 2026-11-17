//! P2-B2: biblioteca portable de postprocesadores y editor transaccional egui.
//! Las instrucciones Start/End son texto del usuario. Ningún perfil certifica
//! la seguridad CNC ni se utiliza todavía para exportar NC desde la interfaz.

use cam_libre_visor::nc_post::{
    PostLineEnding, PostLineNumbers, PostProcessorProfile, PostSpindleDirection, PostUnits,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const LIBRARY_VERSION: u32 = 1;
const MAX_LIBRARY_BYTES: usize = 2 * 1024 * 1024;
const MAX_USER_PROFILES: usize = 64;
const BUILTIN_ID: &str = "grbl_3axis_mm_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostLibrary {
    version: u32,
    profiles: Vec<PostProcessorProfile>,
}

impl Default for PostLibrary {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            profiles: Vec::new(),
        }
    }
}

impl PostLibrary {
    fn validate(&self) -> Result<(), String> {
        if self.version != LIBRARY_VERSION {
            return Err("Versión desconocida de la biblioteca de postprocesadores".into());
        }
        if self.profiles.len() > MAX_USER_PROFILES {
            return Err("Se supera el máximo de 64 perfiles de usuario".into());
        }
        let mut seen = HashSet::new();
        for profile in &self.profiles {
            profile
                .validate()
                .map_err(|err| format!("Perfil '{}': {err}", profile.name))?;
            if profile.id == BUILTIN_ID || !seen.insert(profile.id.as_str()) {
                return Err("Identificador reservado o duplicado en biblioteca NC".into());
            }
        }
        Ok(())
    }
}

/// Fichero ausente: biblioteca de usuario vacía en memoria. Nunca escribir
/// automáticamente ni intentar reparar JSON dañado sin intervención del usuario.
fn read_library(path: &Path) -> Result<PostLibrary, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(PostLibrary::default()),
        Err(err) => return Err(format!("No se puede acceder a postprocessors.json: {err}")),
    };
    if metadata.len() > MAX_LIBRARY_BYTES as u64 {
        return Err("postprocessors.json excede el límite de 2 MiB".into());
    }
    let data =
        fs::read(path).map_err(|err| format!("No se puede leer postprocessors.json: {err}"))?;
    if data.len() > MAX_LIBRARY_BYTES {
        return Err("postprocessors.json excede el límite de 2 MiB".into());
    }
    let library: PostLibrary = serde_json::from_slice(&data)
        .map_err(|err| format!("JSON inválido en postprocessors.json: {err}"))?;
    library.validate()?;
    Ok(library)
}

fn write_library(path: &Path, library: &PostLibrary) -> Result<(), String> {
    library.validate()?;
    let bytes = serde_json::to_vec_pretty(library)
        .map_err(|err| format!("No se puede serializar biblioteca NC: {err}"))?;
    if bytes.len() > MAX_LIBRARY_BYTES {
        return Err("La biblioteca NC supera el límite de 2 MiB".into());
    }
    crate::portable::atomic_write(path, &bytes)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum EditorTab {
    #[default]
    General,
    StartEnd,
    Movement,
    FeedSpindle,
}

pub struct PostUi {
    pub open: bool,
    path: Option<PathBuf>,
    library: PostLibrary,
    selected_id: String,
    draft: Option<PostProcessorProfile>,
    tab: EditorTab,
    error: String,
    blocked: bool,
    confirm_delete: bool,
}

impl PostUi {
    pub fn load() -> Self {
        let mut error = String::new();
        let path = match crate::portable::config_file("postprocessors.json") {
            Ok(path) => Some(path),
            Err(err) => {
                error = err;
                None
            }
        };
        let mut blocked = path.is_none();
        let library = match path.as_ref() {
            Some(path) => match read_library(path) {
                Ok(library) => library,
                Err(err) => {
                    blocked = true;
                    error = err;
                    PostLibrary::default()
                }
            },
            None => PostLibrary::default(),
        };
        Self {
            open: false,
            path,
            library,
            selected_id: BUILTIN_ID.into(),
            draft: None,
            tab: EditorTab::General,
            error,
            blocked,
            confirm_delete: false,
        }
    }

    /// Solo perfiles guardados; los cambios del borrador no se exportan.
    /// Ante una biblioteca ilegible, no se utiliza su contenido en postprocesado.
    pub fn profiles_for_export(&self) -> Vec<PostProcessorProfile> {
        if self.blocked {
            return Vec::new();
        }
        let mut profiles = vec![PostProcessorProfile::grbl_3axis_mm()];
        profiles.extend(self.library.profiles.iter().cloned());
        profiles
    }
    fn selected_profile(&self) -> Option<PostProcessorProfile> {
        if self.selected_id == BUILTIN_ID {
            Some(PostProcessorProfile::grbl_3axis_mm())
        } else {
            self.library
                .profiles
                .iter()
                .find(|p| p.id == self.selected_id)
                .cloned()
        }
    }

    fn switch_to(&mut self, id: &str) {
        if self.draft.is_some() {
            self.error = "Guarda o cancela la edición antes de cambiar de perfil".into();
            return;
        }
        self.selected_id = id.to_owned();
        self.confirm_delete = false;
        self.error.clear();
    }

    fn start_new(&mut self, duplicate: bool) {
        if self.blocked {
            return;
        }
        if self.draft.is_some() {
            self.error = "Guarda o cancela el perfil antes de crear otro".into();
            return;
        }
        let mut profile = if duplicate {
            match self.selected_profile() {
                Some(profile) => profile,
                None => return,
            }
        } else {
            PostProcessorProfile::grbl_3axis_mm()
        };
        profile.id = format!("user_{}", Uuid::new_v4().simple());
        profile.name = if duplicate {
            format!("Copia de {}", profile.name)
        } else {
            "Nuevo postprocesador".into()
        };
        // El perfil no se guarda hasta pulsar Guardar.
        self.draft = Some(profile);
        self.tab = EditorTab::General;
        self.error.clear();
        self.confirm_delete = false;
    }

    fn start_edit(&mut self) {
        if self.blocked || self.draft.is_some() || self.selected_id == BUILTIN_ID {
            return;
        }
        self.draft = self.selected_profile();
        self.tab = EditorTab::General;
        self.error.clear();
    }

    fn save_draft(&mut self) {
        if self.blocked {
            return;
        }
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        let Some(path) = self.path.as_ref() else {
            self.error = "No hay ruta de biblioteca NC".into();
            return;
        };
        let mut candidate = self.library.clone();
        let profile = draft.clone();
        if let Some(existing) = candidate.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            candidate.profiles.push(profile.clone());
        }
        match write_library(path, &candidate) {
            Ok(()) => {
                self.library = candidate;
                self.selected_id = profile.id;
                self.draft = None;
                self.error.clear();
                self.confirm_delete = false;
            }
            Err(err) => self.error = err,
        }
    }

    fn delete_selected(&mut self) {
        if self.blocked || self.draft.is_some() || self.selected_id == BUILTIN_ID {
            return;
        }
        let Some(path) = self.path.as_ref() else {
            self.error = "No hay ruta de biblioteca NC".into();
            return;
        };
        let mut candidate = self.library.clone();
        let before = candidate.profiles.len();
        candidate.profiles.retain(|p| p.id != self.selected_id);
        if before == candidate.profiles.len() {
            self.error = "El perfil seleccionado ya no existe".into();
            return;
        }
        match write_library(path, &candidate) {
            Ok(()) => {
                self.library = candidate;
                self.selected_id = BUILTIN_ID.into();
                self.confirm_delete = false;
                self.error.clear();
            }
            Err(err) => self.error = err,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut window_open = self.open;
        egui::Window::new("Biblioteca de postprocesadores")
            .open(&mut window_open)
            .default_size([1050.0, 700.0])
            .min_width(740.0)
            .resizable(true)
            .show(ctx, |ui| {
                if self.blocked {
                    ui.colored_label(egui::Color32::RED,
                        "La biblioteca NC no está disponible. No se sobrescribirá el archivo existente.");
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
                ui.columns(2, |cols| {
                    cols[0].set_min_width(290.0);
                    cols[0].heading("Perfiles");
                    cols[1].heading("Configuración");
                    cols[0].separator();
                    cols[1].separator();
                    self.list_panel(&mut cols[0]);
                    self.editor_panel(&mut cols[1]);
                });
                ui.separator();
                ui.small("Start/End: comandos libres bajo responsabilidad del usuario. La simulación debe interpretar el NC final; esta ventana no verifica seguridad de máquina.");
                ui.small("Configuración independiente de proyectos: config/postprocessors.json");
            });
        self.open = window_open;
    }

    fn list_panel(&mut self, ui: &mut egui::Ui) {
        let mut entries = vec![(
            BUILTIN_ID.to_owned(),
            "GRBL 1.1 - tres ejes (mm) [incluido]".to_owned(),
        )];
        entries.extend(
            self.library
                .profiles
                .iter()
                .map(|p| (p.id.clone(), p.name.clone())),
        );
        egui::ScrollArea::vertical()
            .max_height(410.0)
            .show(ui, |ui| {
                for (id, name) in entries {
                    if ui.selectable_label(self.selected_id == id, name).clicked() {
                        self.switch_to(&id);
                    }
                }
            });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.blocked && self.draft.is_none(),
                    egui::Button::new("Nuevo"),
                )
                .clicked()
            {
                self.start_new(false);
            }
            if ui
                .add_enabled(
                    !self.blocked && self.draft.is_none(),
                    egui::Button::new("Duplicar"),
                )
                .clicked()
            {
                self.start_new(true);
            }
            if ui
                .add_enabled(
                    !self.blocked && self.draft.is_none() && self.selected_id != BUILTIN_ID,
                    egui::Button::new("Editar"),
                )
                .clicked()
            {
                self.start_edit();
            }
        });
        if self.selected_id == BUILTIN_ID {
            ui.small("Perfil incluido de solo lectura: duplica para personalizar.");
        }
        let can_delete = !self.blocked && self.draft.is_none() && self.selected_id != BUILTIN_ID;
        if can_delete {
            if !self.confirm_delete {
                if ui.button("Eliminar perfil…").clicked() {
                    self.confirm_delete = true;
                }
            } else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "¿Eliminar este perfil de la biblioteca?",
                );
                ui.horizontal(|ui| {
                    if ui.button("Sí, eliminar").clicked() {
                        self.delete_selected();
                    }
                    if ui.button("Cancelar").clicked() {
                        self.confirm_delete = false;
                    }
                });
            }
        }
    }

    fn editor_panel(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = self.draft.as_mut() else {
            if let Some(profile) = self.selected_profile() {
                ui.heading(profile.name);
                ui.label(format!("Controlador: {}", profile.controller));
                ui.label(format!("Extensión: .{}", profile.extension));
                ui.label(format!(
                    "Unidades NC: {}",
                    match profile.units {
                        PostUnits::Millimeters => "mm (G21)",
                        PostUnits::Inches => "inch (G20)",
                    }
                ));
                ui.label(format!(
                    "Decimales XYZ: {} · F: {} · S: {}",
                    profile.xyz_decimals, profile.feed_decimals, profile.spindle_decimals
                ));
                ui.label(format!(
                    "Numeración: {}",
                    if profile.line_numbers.is_some() {
                        "sí"
                    } else {
                        "no"
                    }
                ));
                ui.label(format!(
                    "Husillo automático: {}",
                    if profile.auto_start_spindle {
                        "sí"
                    } else {
                        "no"
                    }
                ));
                ui.separator();
                ui.label("Start (solo lectura)");
                ui.monospace(if profile.start_commands.is_empty() {
                    "(sin instrucciones)"
                } else {
                    &profile.start_commands
                });
                ui.separator();
                ui.label("End (solo lectura)");
                ui.monospace(if profile.end_commands.is_empty() {
                    "(sin instrucciones)"
                } else {
                    &profile.end_commands
                });
            }
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.tab, EditorTab::General, "General");
            ui.selectable_value(&mut self.tab, EditorTab::StartEnd, "Start / End");
            ui.selectable_value(&mut self.tab, EditorTab::Movement, "Movimiento");
            ui.selectable_value(&mut self.tab, EditorTab::FeedSpindle, "Avance / Husillo");
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(510.0).show(ui, |ui| {
            match self.tab {
                EditorTab::General => {
                    ui.label("Nombre del postprocesador");
                    ui.text_edit_singleline(&mut draft.name);
                    ui.label("Identificador (fijo)");
                    ui.monospace(&draft.id);
                    ui.label("Controlador");
                    ui.text_edit_singleline(&mut draft.controller);
                    ui.label("Extensión NC (sin punto)");
                    ui.text_edit_singleline(&mut draft.extension);
                    ui.label("Unidades del G-code (independientes de Opciones)");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut draft.units, PostUnits::Millimeters, "G21 - mm");
                        ui.selectable_value(&mut draft.units, PostUnits::Inches, "G20 - inch");
                    });
                    ui.label("Finales de línea");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut draft.line_ending, PostLineEnding::Lf, "LF");
                        ui.selectable_value(&mut draft.line_ending, PostLineEnding::CrLf, "CRLF");
                    });
                    let mut numbered = draft.line_numbers.is_some();
                    if ui.checkbox(&mut numbered, "Numerar líneas con N").changed() {
                        draft.line_numbers = numbered.then_some(PostLineNumbers { start: 5, step: 5 });
                    }
                    if let Some(numbers) = draft.line_numbers.as_mut() {
                        ui.horizontal(|ui| {
                            ui.label("Inicio N");
                            ui.add(egui::DragValue::new(&mut numbers.start).range(1..=u32::MAX));
                            ui.label("Incremento");
                            ui.add(egui::DragValue::new(&mut numbers.step).range(1..=u32::MAX));
                        });
                    }
                }
                EditorTab::StartEnd => {
                    ui.label("Start: instrucciones antes de la cabecera NC normalizada");
                    ui.add(egui::TextEdit::multiline(&mut draft.start_commands)
                        .desired_rows(9).desired_width(f32::INFINITY).code_editor());
                    ui.separator();
                    ui.label("End: instrucciones después de los movimientos CAM");
                    ui.add(egui::TextEdit::multiline(&mut draft.end_commands)
                        .desired_rows(9).desired_width(f32::INFINITY).code_editor());
                    ui.small("Se guardan sin interpretarlas; se validan caracteres y longitud. Las instrucciones pueden mover físicamente la máquina.");
                }
                EditorTab::Movement => {
                    ui.label("Movimiento de avance G1 · rápido G0 · ejes XYZ · absoluto G90");
                    ui.horizontal(|ui| {
                        ui.label("Decimales XYZ");
                        ui.add(egui::DragValue::new(&mut draft.xyz_decimals).range(0..=8));
                    });
                    ui.checkbox(&mut draft.suppress_unchanged_axes, "Omitir ejes sin cambio");
                    ui.small("La precisión afecta al G-code final. El motor rechazará movimientos que desaparezcan al redondear.");
                }
                EditorTab::FeedSpindle => {
                    ui.horizontal(|ui| {
                        ui.label("Decimales F");
                        ui.add(egui::DragValue::new(&mut draft.feed_decimals).range(0..=6));
                        ui.label("Decimales S");
                        ui.add(egui::DragValue::new(&mut draft.spindle_decimals).range(0..=3));
                    });
                    ui.label("Sentido de husillo");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut draft.spindle_direction,
                            PostSpindleDirection::Clockwise, "M3 horario");
                        ui.selectable_value(&mut draft.spindle_direction,
                            PostSpindleDirection::CounterClockwise, "M4 antihorario");
                    });
                    ui.checkbox(&mut draft.auto_start_spindle, "Emitir M3/M4 automáticamente");
                    ui.checkbox(&mut draft.auto_stop_spindle, "Emitir M5 al final");
                    ui.checkbox(&mut draft.auto_program_end, "Emitir M30 al final");
                    ui.small("F procede de la operación; S procede de la operación. Ningún valor se inventa aquí.");
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Guardar perfil").clicked() {
                self.save_draft();
            }
            if ui.button("Cancelar edición").clicked() {
                self.draft = None;
                self.error.clear();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lagucam-postlib-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("postprocessors.json")
    }

    fn example_profile() -> PostProcessorProfile {
        let mut p = PostProcessorProfile::grbl_3axis_mm();
        p.id = "user_test_001".into();
        p.name = "Personalizado".into();
        p.start_commands = "G21\nG90\n".into();
        p.end_commands = "M5\nM30".into();
        p
    }

    #[test]
    fn missing_library_is_in_memory_only_and_roundtrips() {
        let path = test_path();
        assert_eq!(read_library(&path).unwrap(), PostLibrary::default());
        assert!(!path.exists());
        let library = PostLibrary {
            version: LIBRARY_VERSION,
            profiles: vec![example_profile()],
        };
        write_library(&path, &library).unwrap();
        assert_eq!(read_library(&path).unwrap(), library);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rejects_duplicate_reserved_and_future_profiles() {
        let p = example_profile();
        let mut library = PostLibrary {
            version: LIBRARY_VERSION,
            profiles: vec![p.clone(), p],
        };
        assert!(library.validate().is_err());
        library.profiles.pop();
        library.profiles[0].id = BUILTIN_ID.into();
        assert!(library.validate().is_err());
        library.profiles[0].id = "user_valid_01".into();
        library.version = 99;
        assert!(library.validate().is_err());
    }

    #[test]
    fn corrupt_json_is_not_overwritten_while_loading() {
        let path = test_path();
        fs::write(&path, b"{broken").unwrap();
        assert!(read_library(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn invalid_custom_commands_are_not_saved() {
        let path = test_path();
        let library = PostLibrary {
            version: LIBRARY_VERSION,
            profiles: vec![example_profile()],
        };
        write_library(&path, &library).unwrap();
        let baseline = fs::read(&path).unwrap();
        let mut invalid = library;
        invalid.profiles[0].start_commands = "G90\0M30".into();
        assert!(write_library(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), baseline);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

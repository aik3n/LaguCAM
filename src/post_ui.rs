//! Configuracion global de UN postprocesador, independiente del proyecto.
//! No hay biblioteca ni seleccion de perfiles durante la exportacion.
use cam_libre_visor::nc_post::{
    PostLineEnding, PostLineNumbers, PostProcessorProfile, PostSpindleDirection, PostToolChange,
    PostUnits,
};
use eframe::egui;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 2 * 1024 * 1024;

fn default_fluidnc() -> PostProcessorProfile {
    let mut profile = PostProcessorProfile::grbl_3axis_mm();
    profile.id = "fluidnc_3axis_global_v1".into();
    profile.name = "FluidNC - tres ejes (mm)".into();
    profile.controller = "FluidNC".into();
    profile.tool_change = PostToolChange::M6;
    profile
}

fn read_profile(path: &Path) -> Result<Option<PostProcessorProfile>, String> {
    let meta = match fs::metadata(path) {
        Ok(value) => value,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "No se puede acceder al postprocesador global: {err}"
            ));
        }
    };
    if meta.len() > MAX_BYTES as u64 {
        return Err("El fichero de postprocesador supera 2 MiB".into());
    }
    let data = fs::read(path).map_err(|e| format!("No se puede leer el postprocesador: {e}"))?;
    if data.len() > MAX_BYTES {
        return Err("El fichero de postprocesador supera 2 MiB".into());
    }
    let profile: PostProcessorProfile = serde_json::from_slice(&data).map_err(|e| {
        format!("JSON no valido: se espera UN postprocesador, no una biblioteca: {e}")
    })?;
    profile.validate().map_err(|e| e.to_string())?;
    Ok(Some(profile))
}

fn write_profile(path: &Path, profile: &PostProcessorProfile) -> Result<(), String> {
    profile.validate().map_err(|e| e.to_string())?;
    let data = serde_json::to_vec_pretty(profile).map_err(|e| e.to_string())?;
    if data.len() > MAX_BYTES {
        return Err("Postprocesador demasiado grande".into());
    }
    crate::portable::atomic_write(path, &data)
}

pub struct PostUi {
    pub open: bool,
    path: Option<PathBuf>,
    current: PostProcessorProfile,
    draft: PostProcessorProfile,
    blocked: bool,
    message: String,
}

impl PostUi {
    pub fn load() -> Self {
        let mut message = String::new();
        let path = match crate::portable::config_file("postprocessor.json") {
            Ok(path) => Some(path),
            Err(e) => {
                message = e;
                None
            }
        };
        let mut blocked = path.is_none();
        let current = match path.as_ref().map(|p| read_profile(p)) {
            Some(Ok(Some(p))) => p,
            Some(Ok(None)) => default_fluidnc(),
            Some(Err(e)) => {
                blocked = true;
                message = e;
                default_fluidnc()
            }
            None => default_fluidnc(),
        };
        let draft = current.clone();
        Self {
            open: false,
            path,
            current,
            draft,
            blocked,
            message,
        }
    }

    /// Compatibilidad transitoria con las ventanas NC: siempre devuelve
    /// exactamente el postprocesador global vigente, nunca una biblioteca.
    pub fn profiles_for_export(&self) -> Vec<PostProcessorProfile> {
        if self.blocked {
            Vec::new()
        } else {
            vec![self.current.clone()]
        }
    }

    fn save(&mut self) {
        if self.blocked {
            self.message = "Configuracion bloqueada: corrige el JSON danado; no se sobrescribe automaticamente".into();
            return;
        }
        let Some(path) = &self.path else {
            self.message = "Sin ruta de configuracion".into();
            return;
        };
        match write_profile(path, &self.draft) {
            Ok(()) => {
                self.current = self.draft.clone();
                self.message = "Postprocesador global guardado".into();
            }
            Err(e) => self.message = format!("ERROR: {e}"),
        }
    }

    fn import(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file();
        let Some(path) = path else {
            return;
        };
        match read_profile(&path) {
            Ok(Some(profile)) => {
                self.draft = profile;
                self.message =
                    "Importado al editor. Pulsa Guardar para activar la configuracion global"
                        .into();
            }
            Ok(None) => self.message = "ERROR: Fichero inexistente".into(),
            Err(e) => self.message = format!("ERROR: {e}"),
        }
    }

    fn export(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("lagucam-postprocesador.json")
            .save_file();
        let Some(path) = path else {
            return;
        };
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("json"))
        {
            self.message = "ERROR: La exportacion debe terminar en .json".into();
            return;
        }
        match write_profile(&path, &self.current) {
            Ok(()) => self.message = format!("Exportado: {}", path.display()),
            Err(e) => self.message = format!("ERROR: {e}"),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Postprocesador - configuracion global")
            .open(&mut open).default_size([700.0, 620.0]).min_width(580.0)
            .resizable(true).show(ctx, |ui| {
                ui.small("Un postprocesador activo para todos los proyectos y todas las exportaciones NC.");
                ui.small("Config/postprocessor.json. La antigua postprocessors.json se conserva intacta, sin utilizar.");
                if !self.message.is_empty() {
                    let color = if self.blocked || self.message.starts_with("ERROR") {
                        egui::Color32::LIGHT_RED
                    } else { ui.visuals().text_color() };
                    ui.colored_label(color, &self.message);
                }
                ui.horizontal(|ui| {
                    if ui.button("Importar JSON...").clicked() { self.import(); }
                    if ui.button("Exportar vigente...").clicked() { self.export(); }
                    if ui.button("Restaurar borrador").clicked() {
                        self.draft = self.current.clone();
                        self.message.clear();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Usar plantilla FluidNC").clicked() {
                        self.draft = default_fluidnc();
                    }
                    if ui.button("Usar plantilla GRBL 1.1").clicked() {
                        self.draft = PostProcessorProfile::grbl_3axis_mm();
                    }
                });
                ui.colored_label(egui::Color32::YELLOW,
                    "Tn M6 requiere un controlador o macro M6 compatible y configurado. LaguCAM no puede verificar el cambio fisico.");
                ui.separator();
                let mut save = false;
                egui::ScrollArea::vertical().max_height(470.0).show(ui, |ui| {
                    let draft = &mut self.draft;
                    ui.horizontal(|ui| {
                        ui.label("Nombre"); ui.text_edit_singleline(&mut draft.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Controlador"); ui.text_edit_singleline(&mut draft.controller);
                    });
                    ui.small(format!("Identificador del postprocesador: {}", draft.id));
                    ui.horizontal(|ui| {
                        ui.label("Extension NC"); ui.text_edit_singleline(&mut draft.extension);
                        ui.label("Unidades");
                        ui.selectable_value(&mut draft.units, PostUnits::Millimeters, "G21 mm");
                        ui.selectable_value(&mut draft.units, PostUnits::Inches, "G20 inch");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Decimales XYZ"); ui.add(egui::DragValue::new(&mut draft.xyz_decimals).range(0..=8));
                        ui.label("F"); ui.add(egui::DragValue::new(&mut draft.feed_decimals).range(0..=6));
                        ui.label("S"); ui.add(egui::DragValue::new(&mut draft.spindle_decimals).range(0..=3));
                    });
                    ui.checkbox(&mut draft.suppress_unchanged_axes, "Omitir ejes XYZ sin cambio");
                    ui.horizontal(|ui| {
                        ui.label("Final de linea");
                        ui.selectable_value(&mut draft.line_ending, PostLineEnding::Lf, "LF");
                        ui.selectable_value(&mut draft.line_ending, PostLineEnding::CrLf, "CRLF");
                    });
                    let mut numbered = draft.line_numbers.is_some();
                    if ui.checkbox(&mut numbered, "Numeracion N").changed() {
                        draft.line_numbers = numbered.then_some(PostLineNumbers { start: 5, step: 5 });
                    }
                    if let Some(numbers) = draft.line_numbers.as_mut() {
                        ui.horizontal(|ui| {
                            ui.label("N inicial"); ui.add(egui::DragValue::new(&mut numbers.start).range(1..=u32::MAX));
                            ui.label("Paso"); ui.add(egui::DragValue::new(&mut numbers.step).range(1..=u32::MAX));
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Sentido husillo");
                        ui.selectable_value(&mut draft.spindle_direction, PostSpindleDirection::Clockwise, "M3");
                        ui.selectable_value(&mut draft.spindle_direction, PostSpindleDirection::CounterClockwise, "M4");
                    });
                    ui.checkbox(&mut draft.auto_start_spindle, "Arranque de husillo automatico");
                    ui.checkbox(&mut draft.auto_stop_spindle, "M5 al final");
                    ui.checkbox(&mut draft.auto_program_end, "M30 al final");
                    ui.horizontal(|ui| {
                        ui.label("Cambios de herramienta");
                        ui.selectable_value(&mut draft.tool_change, PostToolChange::Unsupported, "No emitir M6");
                        ui.selectable_value(&mut draft.tool_change, PostToolChange::M6, "Tn M6 en cada operacion con T");
                    });
                    ui.separator();
                    ui.label("Start (antes del programa)");
                    ui.add(egui::TextEdit::multiline(&mut draft.start_commands).desired_rows(4).code_editor());
                    ui.label("End (despues del programa)");
                    ui.add(egui::TextEdit::multiline(&mut draft.end_commands).desired_rows(4).code_editor());
                    ui.small("El NC conjunto no acepta actualmente Start/End con comandos personalizados: no se conoce su efecto sobre la posicion de enlace.");
                });
                ui.separator();
                if ui.add_enabled(!self.blocked, egui::Button::new("Guardar postprocesador global")).clicked() { save = true; }
                if save { self.save(); }
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_profile_roundtrip_and_legacy_default() {
        let path =
            std::env::temp_dir().join(format!("lagucam-postglobal-{}.json", uuid::Uuid::new_v4()));
        assert!(read_profile(&path).unwrap().is_none());
        let profile = default_fluidnc();
        assert_eq!(profile.tool_change, PostToolChange::M6);
        write_profile(&path, &profile).unwrap();
        assert_eq!(read_profile(&path).unwrap(), Some(profile));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn invalid_profile_is_not_overwritten() {
        let path =
            std::env::temp_dir().join(format!("lagucam-postbad-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, b"{broken").unwrap();
        assert!(read_profile(&path).is_err());
        let mut invalid = default_fluidnc();
        invalid.xyz_decimals = 99;
        assert!(write_profile(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_file(path).unwrap();
    }
}

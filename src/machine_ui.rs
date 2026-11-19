//! Configuracion global y portable de la maquina; no pertenece al proyecto.
//! Los recorridos son datos de referencia: no se usan para certificar NC.
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
const MAX_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToolChangeCapability {
    #[default]
    Undefined,
    Manual,
    Automatic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineConfig {
    version: u32,
    name: String,
    controller: String,
    /// Recorridos maximos de referencia X,Y,Z (mm). No son coordenadas G53.
    travel_mm: [Option<f64>; 3],
    max_feed_mm_min: Option<f64>,
    max_spindle_rpm: Option<f64>,
    tool_change: ToolChangeCapability,
}
impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            version: VERSION,
            name: "Maquina sin configurar".into(),
            controller: "FluidNC".into(),
            travel_mm: [None; 3],
            max_feed_mm_min: None,
            max_spindle_rpm: None,
            tool_change: ToolChangeCapability::Undefined,
        }
    }
}
impl MachineConfig {
    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("Version de configuracion de maquina desconocida".into());
        }
        for value in [&self.name, &self.controller] {
            if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err("Nombre o controlador de maquina no valido".into());
            }
        }
        for value in self
            .travel_mm
            .iter()
            .chain([&self.max_feed_mm_min, &self.max_spindle_rpm])
        {
            if let Some(value) = value
                && (!value.is_finite() || *value <= 0.0 || *value > 1_000_000.0)
            {
                return Err("Dato de maquina: se requiere un numero positivo y finito".into());
            }
        }
        Ok(())
    }
}

fn read_machine(path: &Path) -> Result<Option<MachineConfig>, String> {
    let meta = match fs::metadata(path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("No se puede consultar machine.json: {e}")),
    };
    if meta.len() > MAX_BYTES as u64 {
        return Err("machine.json supera 128 KiB".into());
    }
    let data = fs::read(path).map_err(|e| format!("No se puede leer maquina: {e}"))?;
    if data.len() > MAX_BYTES {
        return Err("machine.json supera 128 KiB".into());
    }
    let machine: MachineConfig =
        serde_json::from_slice(&data).map_err(|e| format!("JSON maquina invalido: {e}"))?;
    machine.validate()?;
    Ok(Some(machine))
}

fn write_machine(path: &Path, machine: &MachineConfig) -> Result<(), String> {
    machine.validate()?;
    let data = serde_json::to_vec_pretty(machine).map_err(|e| e.to_string())?;
    if data.len() > MAX_BYTES {
        return Err("Configuracion maquina demasiado grande".into());
    }
    crate::portable::atomic_write(path, &data)
}

fn optional_value(ui: &mut egui::Ui, label: &str, number: &mut Option<f64>, unit: &str) {
    ui.horizontal(|ui| {
        let mut present = number.is_some();
        if ui.checkbox(&mut present, label).changed() {
            *number = if present { Some(0.0) } else { None };
        }
        if let Some(value) = number.as_mut() {
            ui.add(
                egui::DragValue::new(value)
                    .speed(1.0)
                    .range(0.0..=1_000_000.0),
            );
            ui.small(unit);
        } else {
            ui.small("No especificado");
        }
    });
}

pub struct MachineUi {
    pub open: bool,
    path: Option<PathBuf>,
    current: MachineConfig,
    draft: MachineConfig,
    blocked: bool,
    message: String,
}
impl MachineUi {
    pub fn load() -> Self {
        let mut message = String::new();
        let path = match crate::portable::config_file("machine.json") {
            Ok(path) => Some(path),
            Err(e) => {
                message = e;
                None
            }
        };
        let mut blocked = path.is_none();
        let current = match path.as_ref().map(|p| read_machine(p)) {
            Some(Ok(Some(machine))) => machine,
            Some(Ok(None)) => MachineConfig::default(),
            Some(Err(e)) => {
                blocked = true;
                message = e;
                MachineConfig::default()
            }
            None => MachineConfig::default(),
        };
        Self {
            open: false,
            path,
            draft: current.clone(),
            current,
            blocked,
            message,
        }
    }
    pub fn display_name(&self) -> &str {
        &self.current.name
    }
    fn save(&mut self) {
        if self.blocked {
            self.message =
                "Configuracion bloqueada: el JSON anterior esta danado; no se sobrescribe".into();
            return;
        }
        let Some(path) = &self.path else {
            self.message = "Sin ruta para machine.json".into();
            return;
        };
        match write_machine(path, &self.draft) {
            Ok(()) => {
                self.current = self.draft.clone();
                self.message = "Maquina global guardada".into();
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
        match read_machine(&path) {
            Ok(Some(machine)) => {
                self.draft = machine;
                self.message = "Importada al editor; pulsa Guardar para activarla".into();
            }
            Ok(None) => self.message = "ERROR: Fichero inexistente".into(),
            Err(e) => self.message = format!("ERROR: {e}"),
        }
    }
    fn export(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("lagucam-maquina.json")
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
        match write_machine(&path, &self.current) {
            Ok(()) => self.message = format!("Exportado: {}", path.display()),
            Err(e) => self.message = format!("ERROR: {e}"),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Maquina - configuracion global")
            .open(&mut open).default_size([560.0, 520.0]).min_width(500.0)
            .resizable(true).show(ctx, |ui| {
                ui.small("Configuracion global e independiente del postprocesador y del proyecto.");
                ui.small("Los datos son informativos: aun no verifican limites fisicos, fijaciones ni seguridad CNC.");
                if !self.message.is_empty() {
                    ui.colored_label(if self.blocked || self.message.starts_with("ERROR") {
                        egui::Color32::LIGHT_RED
                    } else { ui.visuals().text_color() }, &self.message);
                }
                ui.horizontal(|ui| {
                    if ui.button("Importar JSON...").clicked() { self.import(); }
                    if ui.button("Exportar vigente...").clicked() { self.export(); }
                    if ui.button("Restaurar borrador").clicked() { self.draft = self.current.clone(); }
                });
                ui.separator();
                ui.horizontal(|ui| { ui.label("Nombre"); ui.text_edit_singleline(&mut self.draft.name); });
                ui.horizontal(|ui| { ui.label("Controlador"); ui.text_edit_singleline(&mut self.draft.controller); });
                ui.separator();
                ui.strong("Recorridos de referencia (mm)");
                for (index, name) in ["X", "Y", "Z"].iter().enumerate() {
                    optional_value(ui, name, &mut self.draft.travel_mm[index], "mm");
                }
                ui.separator();
                optional_value(ui, "Avance maximo", &mut self.draft.max_feed_mm_min, "mm/min");
                optional_value(ui, "RPM maximas", &mut self.draft.max_spindle_rpm, "rpm");
                ui.horizontal(|ui| {
                    ui.label("Cambio fisico de herramienta");
                    ui.selectable_value(&mut self.draft.tool_change, ToolChangeCapability::Undefined, "Sin definir");
                    ui.selectable_value(&mut self.draft.tool_change, ToolChangeCapability::Manual, "Manual");
                    ui.selectable_value(&mut self.draft.tool_change, ToolChangeCapability::Automatic, "Automatico");
                });
                ui.small("El procedimiento T/M6 se configura en el postprocesador. Esta capacidad no activa ni cancela M6.");
                ui.separator();
                if ui.add_enabled(!self.blocked, egui::Button::new("Guardar maquina global")).clicked() { self.save(); }
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn machine_json_roundtrip_without_inventing_limits() {
        let path =
            std::env::temp_dir().join(format!("lagucam-machine-{}.json", uuid::Uuid::new_v4()));
        assert_eq!(read_machine(&path).unwrap(), None);
        let original = MachineConfig::default();
        assert_eq!(original.travel_mm, [None; 3]);
        write_machine(&path, &original).unwrap();
        assert_eq!(read_machine(&path).unwrap(), Some(original));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_malformed_and_invalid_machine() {
        let path =
            std::env::temp_dir().join(format!("lagucam-machine-bad-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, b"bad").unwrap();
        assert!(read_machine(&path).is_err());
        let mut machine = MachineConfig::default();
        machine.travel_mm[0] = Some(-1.0);
        assert!(write_machine(&path, &machine).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"bad");
        fs::remove_file(path).unwrap();
    }
}

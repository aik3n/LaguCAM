//! UX-01E.2b: NC conjunto para operaciones con la misma herramienta fisica.
//! No hay cambio automatico de herramienta ni simulacion multiherramienta.
use crate::{
    post_ui::PostUi,
    project::{Project, cam_coordinates::WorkFrame, operations::OperationStatus},
    test_parallel::TestParallelPanel,
    tools::cam_adapter::ToolSignature,
};
use cam_libre_visor::{
    nc_contract::{NcDistanceMode, NcInitialState, NcSpindleMode, NcUnits},
    nc_interpreter::interpret_program,
    nc_post::{
        GeneratedNc, PostBatchOperation, PostMove, PostProcessorProfile, generate_batch_program,
    },
    toolpath_links::PlannedMoveKind,
};
use eframe::egui;

#[derive(Clone)]
struct CombinedPreview {
    generated: GeneratedNc,
    profile_id: String,
    operations: Vec<(usize, ToolSignature)>,
    tool_number: u32,
}

#[derive(Default)]
pub struct NcBatchExportUi {
    open: bool,
    selected_profile: String,
    preview: Option<CombinedPreview>,
    acknowledged: bool,
    message: String,
    refresh: bool,
}

impl NcBatchExportUi {
    pub fn open_for_batch(&mut self) {
        self.open = true;
        self.preview = None;
        self.acknowledged = false;
        self.message.clear();
        self.refresh = true;
    }

    pub fn invalidate(&mut self) {
        self.preview = None;
        self.acknowledged = false;
        self.message = "Vista previa invalidada: vuelve a generar el NC conjunto".into();
    }

    fn generate_current(
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
    ) -> Result<CombinedPreview, String> {
        let model = project.model.as_ref().ok_or("No hay modelo STL")?;
        let stock = project.stock.as_ref().ok_or("Define primero el bruto")?;
        let frame = WorkFrame::new(&project.origin, stock, &model.bounds)
            .map_err(|error| format!("Origen CNC: {error}"))?;
        let mut paths: Vec<Vec<PostMove>> = Vec::new();
        let mut specs = Vec::new();
        let mut identities: Vec<(usize, ToolSignature)> = Vec::new();
        let mut number: Option<u32> = None;
        let mut signature: Option<ToolSignature> = None;
        let mut conditions = Vec::new();
        let mut clearances = Vec::new();

        for (index, operation) in project.operations.items.iter().enumerate() {
            if !operation.active {
                continue;
            }
            if operation.status != OperationStatus::Ready {
                return Err(format!(
                    "'{}': calcula primero esta operacion activa",
                    operation.name
                ));
            }
            let tool_number = operation
                .tool_number
                .ok_or_else(|| format!("'{}': asigna primero un numero T", operation.name))?;
            if !(1..=9999).contains(&tool_number) {
                return Err(format!("'{}': numero T fuera de rango", operation.name));
            }
            if number.is_some_and(|n| n != tool_number) {
                return Err("Las operaciones usan numeros T distintos. No se emite M6 sin definir un procedimiento de cambio en el postprocesador".into());
            }
            number = Some(tool_number);
            let current = panel
                .nc_tool_signature(index)
                .ok_or_else(|| format!("'{}': procedencia CAM no vigente", operation.name))?;
            if operation.tool_id.as_deref() != Some(current.tool_id.as_str()) {
                return Err(format!(
                    "'{}': la fresa ya no coincide con la calculada",
                    operation.name
                ));
            }
            if signature
                .as_ref()
                .is_some_and(|previous| previous != &current)
            {
                return Err("Las operaciones usan fresas fisicas distintas, aunque compartan T. Un mismo T no cambia la geometria de la fresa".into());
            }
            signature = Some(current.clone());
            identities.push((index, current));
            let (linked, cutting) = panel.nc_result(index).ok_or_else(|| {
                format!("'{}': trayectoria CAM invalida o sin corte", operation.name)
            })?;
            let first = linked.moves.first().ok_or("Trayectoria sin inicio")?;
            let last = linked.moves.last().ok_or("Trayectoria sin fin")?;
            if first.kind != PlannedMoveKind::Approach
                || last.kind != PlannedMoveKind::Retract
                || first.from[2] != linked.clearance_z
                || last.to[2] != linked.clearance_z
            {
                return Err(format!(
                    "'{}': no empieza y termina en Z de despeje",
                    operation.name
                ));
            }
            let configured = cutting
                .require_configured()
                .map_err(|error| format!("'{}': configura F y S: {error}", operation.name))?;
            let work = frame.convert(linked).map_err(|error| {
                format!("'{}': conversion al origen CNC: {error}", operation.name)
            })?;
            let clearance = work
                .moves
                .first()
                .ok_or("Trayectoria transformada vacia")?
                .from_work_mm[2];
            let moves = work
                .moves
                .iter()
                .map(|movement| PostMove {
                    kind: movement.kind,
                    from_work_mm: movement.from_work_mm,
                    to_work_mm: movement.to_work_mm,
                    pass_index: movement.pass_index,
                })
                .collect();
            conditions.push(configured);
            clearances.push(clearance);
            paths.push(moves);
        }
        let tool_number = number.ok_or("No hay operaciones activas")?;
        for (i, path) in paths.iter().enumerate() {
            specs.push(PostBatchOperation {
                moves_work_mm: path,
                conditions: conditions[i],
                tool_number,
                clearance_z_work_mm: clearances[i],
            });
        }
        let generated = generate_batch_program(profile, &specs)
            .map_err(|error| format!("Postprocesado conjunto: {error}"))?;
        // Comprobar que la representacion definitiva es interpretable, no
        // fiarse de la trayectoria geometrica previa al redondeo NC.
        let initial = NcInitialState {
            position_work_mm: generated.required_first_move_position_work_mm,
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        };
        interpret_program(&generated.text, initial)
            .map_err(|error| format!("El NC definitivo no se puede interpretar: {error}"))?;
        Ok(CombinedPreview {
            generated,
            profile_id: profile.id.clone(),
            operations: identities,
            tool_number,
        })
    }

    fn save_current(
        &mut self,
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
    ) -> Result<(), String> {
        if !self.acknowledged {
            return Err(
                "Confirma primero la precondicion de posicion y las limitaciones CNC".into(),
            );
        }
        let original = self.preview.as_ref().ok_or("Genera la vista previa")?;
        if original.profile_id != profile.id {
            return Err("El postprocesador ha cambiado; regenera el NC".into());
        }
        let current = Self::generate_current(project, panel, profile)?;
        if original.generated != current.generated || original.operations != current.operations {
            self.invalidate();
            return Err(
                "Las operaciones, sus herramientas o el NC han cambiado; regenera la vista previa"
                    .into(),
            );
        }
        let filename = format!("lagucam_conjunto.{}", profile.extension);
        let path = rfd::FileDialog::new()
            .add_filter("Programa CNC", &[profile.extension.as_str()])
            .set_file_name(&filename)
            .save_file();
        let Some(path) = path else {
            return Ok(());
        };
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(&profile.extension))
        {
            return Err(format!("Selecciona un archivo .{}", profile.extension));
        }
        crate::portable::atomic_write(&path, current.generated.text.as_bytes())?;
        self.message = format!("NC conjunto guardado: {}", path.display());
        Ok(())
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        panel: &TestParallelPanel,
        post: &PostUi,
    ) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("NC conjunto - Operaciones")
            .open(&mut open)
            .default_size([660.0, 500.0])
            .min_width(540.0)
            .resizable(true)
            .show(ctx, |ui| {
                let profiles = post.profiles_for_export();
                if !profiles.iter().any(|profile| profile.id == self.selected_profile) {
                    self.selected_profile = profiles.first().map_or_else(String::new, |p| p.id.clone());
                    self.invalidate();
                }
                let mut selected = self.selected_profile.clone();
                ui.heading("Programa NC conjunto");
                ui.label("Operaciones activas, en el orden del arbol. Una sola herramienta fisica y un solo T.");
                ui.horizontal(|ui| {
                    ui.label("Postprocesador:");
                    let title = profiles.iter().find(|p| p.id == selected)
                        .map_or("Sin perfil", |p| p.name.as_str());
                    egui::ComboBox::from_id_salt("batch_nc_post")
                        .selected_text(title)
                        .show_ui(ui, |ui| {
                            for profile in &profiles {
                                ui.selectable_value(&mut selected, profile.id.clone(), profile.name.as_str());
                            }
                        });
                });
                if selected != self.selected_profile {
                    self.selected_profile = selected;
                    self.invalidate();
                }
                if ui.add_enabled(!profiles.is_empty(), egui::Button::new("Regenerar NC conjunto")).clicked() {
                    self.refresh = true;
                }
                if self.refresh {
                    self.refresh = false;
                    self.acknowledged = false;
                    self.preview = None;
                    if let Some(profile) = profiles.iter().find(|p| p.id == self.selected_profile) {
                        match Self::generate_current(project, panel, profile) {
                            Ok(preview) => {
                                self.preview = Some(preview);
                                self.message = "NC conjunto generado. Revisa antes de exportar".into();
                            }
                            Err(error) => self.message = format!("ERROR: {error}"),
                        }
                    }
                }
                if self.message.starts_with("ERROR") {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.message);
                } else if !self.message.is_empty() {
                    ui.small(&self.message);
                }
                ui.separator();
                if let Some(preview) = &self.preview {
                    let result = &preview.generated;
                    ui.label(format!("{} operaciones · T{} · {} movimientos · {} lineas NC",
                        preview.operations.len(), preview.tool_number, result.cam_move_count, result.text.lines().count()));
                    ui.label(format!("Programa: {} bytes", result.text.len()));
                    let start = result.required_first_move_position_work_mm;
                    ui.colored_label(egui::Color32::YELLOW,
                        format!("PRECONDICION: herramienta T{} ya montada y situada en X{:.3} Y{:.3} Z{:.3} (mm de trabajo) antes del primer movimiento.",
                            preview.tool_number, start[0], start[1], start[2]));
                    ui.small("No se realizan cambios M6. Los enlaces atraviesan XY a la mayor Z de despeje; no estan comprobados contra fijaciones ni limites de maquina.");
                    ui.small("El NC definitivo se ha interpretado, pero no certifica seguridad fisica ni simulacion de material conjunta.");
                    ui.collapsing("Inicio del G-code", |ui| {
                        let excerpt = result.text.lines().take(45).collect::<Vec<_>>().join("\n");
                        egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                            ui.monospace(excerpt);
                        });
                    });
                    ui.separator();
                    ui.checkbox(&mut self.acknowledged,
                        "He comprobado herramienta montada, posicion inicial, despejes y fijaciones");
                    let profile = profiles.iter().find(|p| p.id == self.selected_profile);
                    if ui.add_enabled(self.acknowledged && profile.is_some(), egui::Button::new("Exportar NC conjunto")).clicked()
                        && let Some(profile) = profile
                        && let Err(error) = self.save_current(project, panel, profile)
                        {
                            self.message = format!("ERROR: {error}");
                        }
                }
            });
        self.open = open;
    }
}

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
    operations: Vec<(usize, ToolSignature, Option<u32>)>,
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
        let mut identities: Vec<(usize, ToolSignature, Option<u32>)> = Vec::new();
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
            if operation
                .tool_number
                .is_some_and(|number| !(1..=9999).contains(&number))
            {
                return Err(format!("'{}': numero T fuera de rango", operation.name));
            }
            // La identidad UUID/geometria se comprueba contra SU resultado CAM.
            // No se comparan las fresas de operaciones distintas: T es explicito.
            let current = panel
                .nc_tool_signature(index)
                .ok_or_else(|| format!("'{}': procedencia CAM no vigente", operation.name))?;
            if operation.tool_id.as_deref() != Some(current.tool_id.as_str()) {
                return Err(format!(
                    "'{}': la fresa ya no coincide con la calculada",
                    operation.name
                ));
            }
            identities.push((index, current, operation.tool_number));
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
        if paths.is_empty() {
            return Err("No hay operaciones activas".into());
        }
        let specs: Vec<PostBatchOperation<'_>> = paths
            .iter()
            .enumerate()
            .map(|(i, path)| PostBatchOperation {
                moves_work_mm: path,
                conditions: conditions[i],
                tool_number: identities[i].2,
                clearance_z_work_mm: clearances[i],
            })
            .collect();
        let generated = generate_batch_program(profile, &specs)
            .map_err(|error| format!("Postprocesado conjunto: {error}"))?;
        // Los eventos T y M6 solo representan solicitudes del programa;
        // interpretar el texto no equivale a simular el cambio fisico.
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
        machine_ui: &crate::machine_ui::MachineUi,
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
                ui.heading("Programa NC conjunto");
                ui.label("Operaciones activas en orden. Cada T solicita M6; sin T no se solicita cambio.");
                if let Some(active) = profiles.first() {
                    ui.label(format!("Postprocesador global: {}", active.name));
                } else {
                    ui.colored_label(egui::Color32::LIGHT_RED, "Postprocesador global no disponible");
                }
                ui.small(format!("Maquina global: {} (datos fisicos informativos)", machine_ui.display_name()));                if ui.add_enabled(!profiles.is_empty(), egui::Button::new("Regenerar NC conjunto")).clicked() {
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
                    let requested = preview.operations.iter().filter(|(_, _, t)| t.is_some()).count();
                    ui.label(format!(
                        "{} operaciones · {} solicitudes de cambio T/M6 · {} movimientos · {} lineas NC",
                        preview.operations.len(), requested, result.cam_move_count, result.text.lines().count()
                    ));
                    ui.label(format!("Programa: {} bytes", result.text.len()));
                    let start = result.required_first_move_position_work_mm;
                    ui.colored_label(egui::Color32::YELLOW,
                        format!("PRECONDICION: posicion inicial de trabajo X{:.3} Y{:.3} Z{:.3} mm antes de ejecutar el NC.",
                            start[0], start[1], start[2]));
                    ui.small("Cada operacion con T solicita un cambio, incluso T repetido. Sin T, no se solicita. No se infiere herramienta desde la geometria.");
                    ui.small("El perfil M6 debe gestionar el cambio real y retornar a la posicion de trabajo esperada. No se comprueban fijaciones, limites de maquina ni el comportamiento fisico de M6.");
                    ui.small("El NC definitivo se ha interpretado, pero no certifica seguridad fisica ni simulacion de material conjunta.");
                    ui.separator();
                    ui.strong("Operaciones incluidas en el NC generado:");
                    for (order, (index, _, requested_t)) in preview.operations.iter().enumerate() {
                        let name = project.operations.items.get(*index)
                            .map_or("(operacion eliminada)", |operation| operation.name.as_str());
                        let tool = requested_t.map_or_else(
                            || "sin solicitud de cambio".to_owned(),
                            |number| format!("T{number} M6"),
                        );
                        ui.label(format!("{}. {}  |  {}", order + 1, name, tool));
                    }
                    ui.small("IMPORTANTE: el fragmento inicial solo contiene 45 lineas. Utiliza el fragmento final o exporta el NC para examinarlo completo.");
                    ui.collapsing("Solicitudes T/M6 realmente emitidas en el NC", |ui| {
                        let mut total = 0usize;
                        for (line_number, line) in result.text.lines().enumerate() {
                            if line.trim_end().ends_with(" M6") {
                                ui.monospace(format!("Linea {}: {}", line_number + 1, line));
                                total += 1;
                            }
                        }
                        if total == 0 {
                            ui.small("No hay solicitudes M6 en el texto NC.");
                        }
                    });
                    ui.collapsing("Inicio del G-code (primeras 45 lineas)", |ui| {
                        let excerpt = result.text.lines().take(45).collect::<Vec<_>>().join("\n");
                        egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                            ui.monospace(excerpt);
                        });
                    });
                    ui.collapsing("Final del G-code (ultimas 60 lineas)", |ui| {
                        let mut lines = result.text.lines().rev().take(60).collect::<Vec<_>>();
                        lines.reverse();
                        let excerpt = lines.join("\n");
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

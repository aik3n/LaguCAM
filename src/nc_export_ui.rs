//! P2-B3: previsualizacion textual y exportacion del NC generado.
//!
//! Fuente: resultado CAM validado -> WorkFrame -> nc_post -> fichero NC.
//! El NC no se simula aqui: el interprete independiente llegara en P2-C.
//! El fichero se regenera justo al guardar, sin exportar una vista previa obsoleta.

use crate::tools::cam_adapter::ToolSignature;
use crate::{
    post_ui::PostUi,
    project::{Project, cam_coordinates::WorkFrame, operations::OperationStatus},
    test_parallel::TestParallelPanel,
};
use cam_libre_visor::{
    cam_contract::CuttingConditions,
    nc_contract::{
        NcDistanceMode, NcEventKind, NcInitialState, NcMotion, NcMotionKind, NcSpindleMode,
        NcTrace, NcUnits,
    },
    nc_interpreter::interpret_program,
    nc_post::{GeneratedNc, PostInput, PostMove, PostProcessorProfile, generate_program},
    nc_verify::{
        CutterEnvelope, StockEnvelope, VerificationIssue, VerificationReport, VerificationSeverity,
        verify_trace,
    },
    toolpath_links::PlannedToolpath,
};
use eframe::egui;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct NcPreview {
    operation_index: usize,
    operation_name: String,
    profile_id: String,
    generated: GeneratedNc,
    tool_signature: ToolSignature,
    summary: Result<NcSummary, String>,
    verification: Option<Result<NcVerification, String>>,
    source_frame: WorkFrame,
    model_session: u64,
}

/// Magnitudes obtenidas al interpretar el G-code definitivo.
/// El tiempo G1 no incluye G0, aceleraciones ni tiempos auxiliares.
#[derive(Clone, Debug, PartialEq)]
struct NcSummary {
    rapid_moves: usize,
    feed_moves: usize,
    rapid_mm: f64,
    feed_mm: f64,
    feed_minutes: f64,
}

fn summarize_interpreted_nc(
    generated: &GeneratedNc,
    profile: &PostProcessorProfile,
) -> Result<NcSummary, String> {
    // El origen CAM es la posicion exigida antes del primer movimiento CAM.
    // Un bloque Start personalizado puede haber desplazado la herramienta.
    if !profile.start_commands.trim().is_empty() {
        return Err("Start personalizado: posicion inicial anterior a Start desconocida; no se calculan metricas NC automaticamente".into());
    }
    let initial = NcInitialState {
        position_work_mm: generated.required_first_move_position_work_mm,
        units: NcUnits::Millimeters,
        distance_mode: NcDistanceMode::Absolute,
        feed_mm_min: None,
        spindle_rpm: None,
        spindle_mode: NcSpindleMode::Stopped,
        tool_number: None,
    };
    let trace = interpret_program(&generated.text, initial)
        .map_err(|error| format!("No se pudo interpretar el NC definitivo: {error}"))?;
    let mut summary = NcSummary {
        rapid_moves: 0,
        feed_moves: 0,
        rapid_mm: 0.0,
        feed_mm: 0.0,
        feed_minutes: 0.0,
    };
    for event in trace.events {
        let NcEventKind::Motion(motion) = event.kind else {
            continue;
        };
        let distance = (0..3)
            .map(|axis| (motion.to_work_mm[axis] - motion.from_work_mm[axis]).powi(2))
            .sum::<f64>()
            .sqrt();
        if !distance.is_finite() {
            return Err("Distancia NC fuera del rango numerico".into());
        }
        match motion.kind {
            NcMotionKind::Rapid => {
                summary.rapid_moves += 1;
                summary.rapid_mm += distance;
            }
            NcMotionKind::LinearFeed => {
                let feed = motion
                    .effective_feed_mm_min
                    .ok_or("Movimiento G1 sin avance modal F")?;
                if !feed.is_finite() || feed <= 0.0 {
                    return Err("Avance NC invalido".into());
                }
                summary.feed_moves += 1;
                summary.feed_mm += distance;
                summary.feed_minutes += distance / feed;
            }
        }
    }
    if !summary.rapid_mm.is_finite()
        || !summary.feed_mm.is_finite()
        || !summary.feed_minutes.is_finite()
    {
        return Err("Resumen NC fuera del rango numerico".into());
    }
    Ok(summary)
}

/// La verificacion solo retiene los movimientos citados en los avisos visibles.
#[derive(Clone)]
struct NcVerification {
    report: VerificationReport,
    issue_motions: Vec<Option<NcMotion>>,
}

/// El indice del verificador es un indice de MOVIMIENTO, no de evento NC.
fn issue_motion_targets(trace: &NcTrace, issues: &[VerificationIssue]) -> Vec<Option<NcMotion>> {
    let wanted: HashSet<usize> = issues.iter().filter_map(|issue| issue.move_index).collect();
    let mut found = HashMap::with_capacity(wanted.len());
    let mut move_index = 0;
    for event in &trace.events {
        if let NcEventKind::Motion(motion) = event.kind {
            if wanted.contains(&move_index) {
                found.insert(move_index, motion);
            }
            move_index += 1;
        }
    }
    issues
        .iter()
        .map(|issue| {
            issue
                .move_index
                .and_then(|index| found.get(&index).copied())
        })
        .collect()
}

fn verify_generated_nc(
    preview: &NcPreview,
    project: &Project,
    library: &crate::tools::model::ToolLibrary,
) -> Result<NcVerification, String> {
    // Un Start personalizado puede desplazar el punto inicial. No se inventa.
    if preview.summary.is_err() {
        return Err("No se ha podido establecer una posicion inicial fiable para interpretar todo el NC. Revisa Start y el informe de metricas.".into());
    }
    let op = project
        .operations
        .items
        .get(preview.operation_index)
        .ok_or("La operacion de origen ya no existe")?;
    if op.status != OperationStatus::Ready
        || !crate::tools::cam_adapter::signature_current(
            library,
            op.tool_id.as_deref(),
            &preview.tool_signature,
        )
    {
        return Err("La operacion o la fresa han cambiado desde que se genero el NC".into());
    }
    if project.model_session != preview.model_session {
        return Err("El modelo STL ha cambiado desde que se genero el NC".into());
    }
    let model = project.model.as_ref().ok_or("No hay modelo STL")?;
    let stock = project.stock.as_ref().ok_or("No hay bruto")?;
    let frame = WorkFrame::new(&project.origin, stock, &model.bounds)
        .map_err(|e| format!("Origen CNC no verificable: {e}"))?;
    if frame != preview.source_frame {
        return Err("Ha cambiado el bruto o el origen CNC: regenera el NC".into());
    }
    let tool = library
        .get(&preview.tool_signature.tool_id)
        .ok_or("La fresa de origen CAM no esta en la biblioteca")?;
    tool.validate()?;
    let trace = interpret_program(
        &preview.generated.text,
        NcInitialState {
            position_work_mm: preview.generated.required_first_move_position_work_mm,
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        },
    )
    .map_err(|e| format!("Interpretacion del NC: {e}"))?;
    let stock_max = stock.max().ok_or("Limites del bruto no validos")?;
    let report = verify_trace(
        &trace,
        StockEnvelope {
            min_mm: stock.min,
            max_mm: stock_max,
        },
        CutterEnvelope {
            radius_mm: tool.geometry.diameter_mm() * 0.5,
            cutting_length_mm: tool.cutting_length_mm,
        },
        |position| {
            project
                .origin
                .work_to_model(stock, &model.bounds, position)
                .map_err(str::to_owned)
        },
    )?;
    let issue_motions = issue_motion_targets(&trace, &report.issues);
    Ok(NcVerification {
        report,
        issue_motions,
    })
}

#[derive(Default)]
pub struct NcExportUi {
    pub open: bool,
    selected_operation: Option<usize>,
    selected_profile: String,
    preview: Option<NcPreview>,
    pending_preview: bool,
    focus_issue: Option<usize>,
    message: String,
    pending_playback: Option<(String, [f64; 3], ToolSignature)>,
}

/// Contrato puro: el post solo recibe coordenadas de trabajo en mm.
/// F/S NO reciben valores por defecto: la operacion debe configurarlos.
fn build_nc(
    frame: &WorkFrame,
    linked: &PlannedToolpath,
    conditions: CuttingConditions,
    profile: &PostProcessorProfile,
) -> Result<GeneratedNc, String> {
    let configured = conditions
        .require_configured()
        .map_err(|error| format!("Configura F y S en la estrategia antes de exportar: {error}"))?;
    let work = frame.convert(linked).map_err(|error| {
        format!("No se pueden convertir los movimientos al origen CNC: {error}")
    })?;
    let movements: Vec<PostMove> = work
        .moves
        .iter()
        .map(|movement| PostMove {
            kind: movement.kind,
            from_work_mm: movement.from_work_mm,
            to_work_mm: movement.to_work_mm,
            pass_index: movement.pass_index,
        })
        .collect();
    generate_program(
        profile,
        PostInput {
            moves_work_mm: &movements,
            conditions: configured,
        },
    )
    .map_err(|error| format!("No se puede generar el programa NC: {error}"))
}

/// Nombre sugerido. No acepta rutas ni separadores procedentes del nombre de la operacion.
fn suggested_filename(name: &str, extension: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let stem = if stem.trim_matches('_').is_empty() {
        "lagucam_operacion"
    } else {
        &stem
    };
    format!("{stem}.{extension}")
}

#[cfg(test)]
fn preview_excerpt(text: &str) -> String {
    const HEAD: usize = 65;
    const TAIL: usize = 22;
    let count = text.lines().count();
    if count <= HEAD + TAIL {
        return text.to_owned();
    }
    let head = text.lines().take(HEAD).collect::<Vec<_>>().join("\n");
    let mut tail = text.lines().rev().take(TAIL).collect::<Vec<_>>();
    tail.reverse();
    format!(
        "{head}\n\n... [se omiten {} lineas de la vista; el fichero se guarda completo] ...\n\n{}\n",
        count - HEAD - TAIL,
        tail.join("\n")
    )
}

impl NcExportUi {
    /// Una sola verificacion por generacion. Nunca recalcular por cada frame.
    pub fn refresh_verification(
        &mut self,
        project: &Project,
        library: &crate::tools::model::ToolLibrary,
    ) {
        if !self.open {
            return;
        }
        let Some(preview) = self.preview.as_mut() else {
            return;
        };
        if preview.verification.is_none() {
            preview.verification = Some(verify_generated_nc(preview, project, library));
        }
    }

    /// Marca el movimiento asociado a la alerta directamente sobre el visor 3D.
    /// No carga otro NC en el simulador ni altera la reproduccion en marcha.
    pub fn paint_issue_focus(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        project: &Project,
        camera: crate::nc_tool_overlay::Camera,
    ) {
        if !self.open {
            return;
        }
        let Some(issue_index) = self.focus_issue else {
            return;
        };
        let Some(preview) = self.preview.as_ref() else {
            return;
        };
        let Some(Ok(verification)) = preview.verification.as_ref() else {
            return;
        };
        let Some(Some(motion)) = verification.issue_motions.get(issue_index) else {
            return;
        };
        let Some(issue) = verification.report.issues.get(issue_index) else {
            return;
        };
        let Some(model) = project.model.as_ref() else {
            return;
        };
        let Some(stock) = project.stock.as_ref() else {
            return;
        };
        if project.model_session != preview.model_session
            || project
                .operations
                .items
                .get(preview.operation_index)
                .is_none_or(|op| {
                    op.status != OperationStatus::Ready
                        || op.tool_id.as_deref() != Some(preview.tool_signature.tool_id.as_str())
                })
            || !WorkFrame::new(&project.origin, stock, &model.bounds)
                .is_ok_and(|frame| frame == preview.source_frame)
        {
            return;
        }
        let mvp = crate::navigation::view_projection_mode(
            camera.yaw,
            camera.pitch,
            camera.distance,
            rect.width() / rect.height().max(1.0),
            camera.pan,
            camera.projection,
        );
        let screen_point = |work: [f64; 3]| -> Option<egui::Pos2> {
            let model_xyz = project
                .origin
                .work_to_model(stock, &model.bounds, work)
                .ok()?;
            let position = model.visual_point(model_xyz)?;
            let clip = mvp * position.extend(1.0);
            if !clip.is_finite() || clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            if !ndc.is_finite()
                || ndc.x.abs() > 1.0
                || ndc.y.abs() > 1.0
                || !(-1.0..=1.0).contains(&ndc.z)
            {
                return None;
            }
            Some(
                rect.center()
                    + egui::vec2(ndc.x * rect.width() * 0.5, -ndc.y * rect.height() * 0.5),
            )
        };
        let (Some(from), Some(to)) = (
            screen_point(motion.from_work_mm),
            screen_point(motion.to_work_mm),
        ) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        painter.line_segment([from, to], egui::Stroke::new(6.0, egui::Color32::BLACK));
        painter.line_segment([from, to], egui::Stroke::new(3.0, egui::Color32::YELLOW));
        for point in [from, to] {
            painter.circle_filled(point, 4.0, egui::Color32::YELLOW);
            painter.circle_stroke(point, 5.5, egui::Stroke::new(1.5, egui::Color32::BLACK));
        }
        let label_position = from + egui::vec2(9.0, -10.0);
        painter.text(
            label_position,
            egui::Align2::LEFT_BOTTOM,
            format!("Aviso NC · L{}", issue.source_line),
            egui::FontId::proportional(13.0),
            egui::Color32::YELLOW,
        );
    }

    /// Abrir resultados NC de la operacion seleccionada y generar automaticamente.
    pub fn open_for_operation(&mut self, index: usize) {
        self.selected_operation = Some(index);
        self.preview = None;
        self.pending_preview = true;
        self.message.clear();
        self.open = true;
    }

    /// Construir el NC final para reproduccion, sin seleccionar archivos externos.
    pub fn playback_for_operation(
        &mut self,
        index: usize,
        project: &Project,
        panel: &TestParallelPanel,
        post_ui: &PostUi,
    ) -> Result<(String, [f64; 3], ToolSignature), String> {
        let profiles = post_ui.profiles_for_export();
        let profile = profiles
            .iter()
            .find(|p| p.id == self.selected_profile)
            .or_else(|| profiles.first())
            .ok_or("No hay postprocesador disponible")?;
        if !profile.start_commands.trim().is_empty() {
            return Err("Simulacion automatica no disponible con Start personalizado; puede mover la herramienta antes de CAM".into());
        }
        self.selected_profile = profile.id.clone();
        let preview = self.generate_current(project, panel, profile, index)?;
        let generated = preview.generated;
        Ok((
            generated.text,
            generated.required_first_move_position_work_mm,
            preview.tool_signature,
        ))
    }

    /// La seleccion interna por indice del exportador anterior no puede apuntar
    /// a otra operacion despues de mover filas en el arbol.
    pub fn operation_reordered(&mut self) {
        self.selected_operation = None;
        self.preview = None;
        self.pending_playback = None;
        self.message.clear();
    }

    /// Devuelve una unica peticion; se envia el texto NC completo, no la trayectoria CAM.
    pub fn take_playback_request(&mut self) -> Option<(String, [f64; 3], ToolSignature)> {
        self.pending_playback.take()
    }

    fn generate_current(
        &self,
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
        index: usize,
    ) -> Result<NcPreview, String> {
        let operation = project
            .operations
            .items
            .get(index)
            .ok_or("La operacion elegida ya no existe")?;
        if operation.status != OperationStatus::Ready {
            return Err(
                "La operacion no dispone de una trayectoria CAM lista; vuelve a calcularla".into(),
            );
        }
        let (linked, conditions) = panel
            .nc_result(index)
            .ok_or("El resultado CAM ha sido invalidado o no contiene cortes")?;
        let tool_signature = panel
            .nc_tool_signature(index)
            .ok_or("La operacion ya no conserva procedencia de herramienta CAM")?;
        if operation.tool_id.as_deref() != Some(tool_signature.tool_id.as_str()) {
            return Err(
                "La herramienta asignada no coincide con el calculo CAM; vuelve a calcular".into(),
            );
        }
        let model = project.model.as_ref().ok_or("No hay modelo cargado")?;
        let stock = project.stock.as_ref().ok_or("No hay bruto definido")?;
        let frame = WorkFrame::new(&project.origin, stock, &model.bounds).map_err(|error| {
            format!("Origen CNC no admitido por este postprocesador de tres ejes: {error}")
        })?;
        let generated = build_nc(&frame, linked, conditions, profile)?;
        let summary = summarize_interpreted_nc(&generated, profile);
        Ok(NcPreview {
            operation_index: index,
            operation_name: operation.name.clone(),
            profile_id: profile.id.clone(),
            generated,
            tool_signature,
            summary,
            verification: None,
            source_frame: frame,
            model_session: project.model_session,
        })
    }

    fn save_current(
        &mut self,
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
        index: usize,
    ) -> Result<(), String> {
        let previous = self
            .preview
            .as_ref()
            .ok_or("Genera primero la vista previa")?;
        if previous.operation_index != index || previous.profile_id != profile.id {
            return Err("La operacion o el perfil han cambiado; regenera la vista previa".into());
        }
        let current = self.generate_current(project, panel, profile, index)?;
        if current.operation_name != previous.operation_name
            || current.tool_signature != previous.tool_signature
            || current.generated != previous.generated
        {
            self.preview = None;
            return Err("El NC ha cambiado desde la vista previa (origen, CAM o perfil). Regenera la vista previa antes de exportar".into());
        }
        let suggestion = suggested_filename(&current.operation_name, &profile.extension);
        let path = rfd::FileDialog::new()
            .add_filter("Programa CNC", &[profile.extension.as_str()])
            .set_file_name(&suggestion)
            .save_file();
        let Some(path) = path else {
            return Ok(());
        };
        if !path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(&profile.extension))
        {
            return Err(format!(
                "Selecciona un archivo con extension .{}",
                profile.extension
            ));
        }
        // No se toca el STL ni .lagucam. Se escribe un temporal completo antes
        // de reemplazar el destino seleccionado (si el usuario confirma).
        crate::portable::atomic_write(&path, current.generated.text.as_bytes())?;
        self.message = format!("NC exportado: {}", path.display());
        Ok(())
    }

    /// Ventana flotante de compatibilidad; comparte exactamente la misma UI.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        panel: &TestParallelPanel,
        post_ui: &PostUi,
        machine_ui: &crate::machine_ui::MachineUi,
    ) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Resultados NC")
            .open(&mut open)
            .default_size([690.0, 520.0])
            .min_width(570.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.show_embedded(ui, project, panel, post_ui, machine_ui)
            });
        self.open = open;
    }

    /// Contenido compartido por ventana y panel de mecanizado.
    pub fn show_embedded(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        panel: &TestParallelPanel,
        post_ui: &PostUi,
        machine_ui: &crate::machine_ui::MachineUi,
    ) {
        let available: Vec<(usize, String)> = project
            .operations
            .items
            .iter()
            .enumerate()
            .filter(|(i, op)| op.status == OperationStatus::Ready && panel.nc_result(*i).is_some())
            .map(|(i, op)| (i, op.name.clone()))
            .collect();
        let profiles = post_ui.profiles_for_export();
        // La ventana conserva el ultimo programa generado hasta que se regenere.
        // Cambiar la seleccion del arbol no reemplaza ni invalida la ventana.
        if !available
            .iter()
            .any(|(i, _)| Some(*i) == self.selected_operation)
        {
            self.selected_operation = None;
            self.preview = None;
        }
        if !profiles.iter().any(|p| p.id == self.selected_profile) {
            self.selected_profile = profiles.first().map_or_else(String::new, |p| p.id.clone());
            self.preview = None;
        }

        let operation = self.selected_operation;
        let profile_id = self.selected_profile.clone();
        let mut preview_requested = std::mem::take(&mut self.pending_preview);
        let mut export_requested = false;
        let mut playback_requested = false;

        ui.heading("Resultados del programa NC");
        if available.is_empty() {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "\u{2716} No hay una operacion CAM lista para exportar.",
            );
        }
        if let Some(index) = operation {
            if let Some(op) = project.operations.items.get(index) {
                ui.label(format!("Operacion: {}", op.name));
            }
        } else if !available.is_empty() {
            ui.label("Selecciona una operacion calculada en el arbol del proyecto.");
        }
        if let Some(active) = profiles.first() {
            ui.label(format!("Postprocesador global: {}", active.name));
        }
        ui.small(format!("Maquina global: {}", machine_ui.display_name()));
        if profiles.is_empty() {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "\u{2716} Postprocesador global no disponible. Revisa su configuracion.",
            );
        }
        if let Some(profile) = profiles.iter().find(|p| p.id == profile_id) {
            ui.label(format!(
                "Controlador: {} · NC: {} · extension: .{} · XYZ: {} decimales",
                profile.controller,
                profile.units.code(),
                profile.extension,
                profile.xyz_decimals
            ));
            if !profile.start_commands.is_empty() || !profile.end_commands.is_empty() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "\u{26a0} Start/End personalizados pueden introducir movimientos adicionales.",
                );
            }
        }
        ui.separator();
        let selected_matches_preview = self
            .preview
            .as_ref()
            .is_some_and(|p| Some(p.operation_index) == operation && p.profile_id == profile_id);
        if ui
            .add_enabled(
                operation.is_some() && !profiles.is_empty(),
                egui::Button::new("Regenerar NC"),
            )
            .clicked()
        {
            preview_requested = true;
        }
        if selected_matches_preview && let Some(preview) = self.preview.as_ref() {
            let generated = &preview.generated;
            ui.label(format!(
                "{} movimientos CAM · {} lineas NC · {} bytes",
                generated.cam_move_count,
                generated.text.lines().count(),
                generated.text.len()
            ));
            ui.separator();
            match &preview.summary {
                Ok(summary) => {
                    ui.label(format!(
                        "G0: {} movimientos · {:.2} mm     G1: {} movimientos · {:.2} mm",
                        summary.rapid_moves, summary.rapid_mm, summary.feed_moves, summary.feed_mm,
                    ));
                    ui.label(format!(
                        "Tiempo programado G1: {:.2} min ({:.1} s)",
                        summary.feed_minutes,
                        summary.feed_minutes * 60.0,
                    ))
                    .on_hover_text(
                        "Tiempo parcial G1: no incluye G0, aceleraciones ni tiempos auxiliares.",
                    );
                }
                Err(reason) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("\u{26a0} Metricas NC no disponibles: {reason}"),
                    );
                }
            }
            ui.separator();
            ui.strong("Verificacion preliminar del NC interpretado")
                .on_hover_text("Evaluacion del bruto original; no verifica fijaciones, portaherramientas ni limites fisicos de maquina.");
            match preview.verification.as_ref() {
                None => {
                    ui.small("Preparando avisos de verificacion...");
                    ui.ctx().request_repaint();
                }
                Some(Err(reason)) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("\u{26a0} Verificacion no disponible: {reason}"),
                    );
                }
                Some(Ok(verification)) => {
                    let report = &verification.report;
                    ui.label(format!(
                        "{} movimientos examinados · {} incidencias ({} visibles)",
                        report.checked_moves,
                        report.total_issues(),
                        report.issues.len(),
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for (issue_index, issue) in report.issues.iter().enumerate() {
                                let (symbol, color) = match issue.severity {
                                    VerificationSeverity::Warning => {
                                        ("\u{26a0}", egui::Color32::YELLOW)
                                    }
                                    VerificationSeverity::Info => {
                                        ("\u{24d8}", ui.visuals().text_color())
                                    }
                                };
                                let description = format!(
                                    "{symbol} L{} · {}",
                                    issue.source_line,
                                    issue.code.label()
                                );
                                if verification.issue_motions[issue_index].is_some() {
                                    let selected = self.focus_issue == Some(issue_index);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(description).color(color),
                                        )
                                        .on_hover_text("Resaltar este movimiento en el visor 3D")
                                        .clicked()
                                    {
                                        self.focus_issue = Some(issue_index);
                                        ui.ctx().request_repaint();
                                    }
                                } else {
                                    ui.label(egui::RichText::new(description).color(color));
                                }
                            }
                        });
                    if report.omitted_issues > 0 {
                        ui.small(format!(
                            "{} avisos adicionales no mostrados.",
                            report.omitted_issues
                        ));
                    }
                }
            }
            let xyz = generated.required_first_move_position_work_mm;
            ui.label(format!(
                "\u{24d8} Posicion inicial requerida antes del primer movimiento CAM: X{:.6} Y{:.6} Z{:.6} mm (coordenadas de trabajo).",
                xyz[0], xyz[1], xyz[2]
            ));
            ui.separator();
            ui.collapsing("Diagnostico avanzado", |ui| {
                ui.small(format!(
                    "Herramienta CAM (UUID): {}",
                    preview.tool_signature.tool_id
                ));
                if ui.button("Reproductor NC tecnico").clicked() {
                    playback_requested = true;
                }
            });
            if ui.button("Exportar archivo NC...").clicked() {
                export_requested = true;
            }
        }
        if !self.message.is_empty() {
            ui.separator();
            if self.message.starts_with("ERROR:") {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("\u{2716} {}", self.message),
                );
            } else {
                ui.small(&self.message);
            }
        }
        if operation != self.selected_operation || profile_id != self.selected_profile {
            self.preview = None;
            self.message.clear();
        }
        self.selected_operation = operation;
        self.selected_profile = profile_id;

        if preview_requested {
            let result = self
                .selected_operation
                .ok_or_else(|| "Selecciona una operacion CAM calculada".to_owned())
                .and_then(|i| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "Selecciona un postprocesador guardado".to_owned())?;
                    self.generate_current(project, panel, profile, i)
                });
            match result {
                Ok(preview) => {
                    self.focus_issue = None;
                    self.preview = Some(preview);
                    self.message.clear();
                }
                Err(error) => {
                    self.preview = None;
                    self.message = format!("ERROR: {error}");
                }
            }
        }
        if playback_requested {
            // Regenerar antes de enviar, igual que al exportar. Un preview obsoleto se rechaza.
            let result = self
                .selected_operation
                .ok_or_else(|| "No hay operacion seleccionada".to_owned())
                .and_then(|index| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "No hay perfil guardado seleccionado".to_owned())?;
                    let current = self.generate_current(project, panel, profile, index)?;
                    let previous = self
                        .preview
                        .as_ref()
                        .ok_or_else(|| "Genera primero la vista previa NC".to_owned())?;
                    if current.operation_index != previous.operation_index
                        || current.operation_name != previous.operation_name
                        || current.profile_id != previous.profile_id
                        || current.tool_signature != previous.tool_signature
                        || current.generated != previous.generated
                    {
                        return Err(
                            "La vista NC ha quedado obsoleta; regenerala antes de reproducir"
                                .to_owned(),
                        );
                    }
                    Ok((
                        current.generated.text.clone(),
                        current.generated.required_first_move_position_work_mm,
                        current.tool_signature.clone(),
                    ))
                });
            match result {
                Ok(request) => {
                    self.pending_playback = Some(request);
                    self.message = "NC enviado al reproductor.".into();
                }
                Err(error) => {
                    self.pending_playback = None;
                    self.preview = None;
                    self.message = format!("ERROR: {error}");
                }
            }
        }
        if export_requested {
            let result = self
                .selected_operation
                .ok_or_else(|| "No hay operacion seleccionada".to_owned())
                .and_then(|index| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "El perfil ha desaparecido".to_owned())?;
                    self.save_current(project, panel, profile, index)
                });
            if let Err(error) = result {
                self.message = format!("ERROR: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        project::{origin::WorkOrigin, stock::Stock},
        stl::Bounds,
    };
    use cam_libre_visor::{
        nc_post::PostUnits,
        toolpath_links::{PlannedMove, PlannedMoveKind},
    };

    #[test]
    fn final_nc_summary_measures_g0_g1_and_programmed_feed_time() {
        let profile = PostProcessorProfile::grbl_3axis_mm();
        let generated = build_nc(&frame(), &path(), conditions(), &profile).unwrap();
        let report = summarize_interpreted_nc(&generated, &profile).unwrap();
        assert_eq!(report.rapid_moves, 1);
        assert_eq!(report.feed_moves, 1);
        assert!((report.rapid_mm - 25.4).abs() < 1.0e-6);
        assert!((report.feed_mm - 12.7).abs() < 1.0e-6);
        assert!((report.feed_minutes - 0.05).abs() < 1.0e-6);
    }

    #[test]
    fn verification_focus_maps_motion_index_not_modal_event_index() {
        let profile = PostProcessorProfile::grbl_3axis_mm();
        let generated = build_nc(&frame(), &path(), conditions(), &profile).unwrap();
        let trace = interpret_program(
            &generated.text,
            NcInitialState {
                position_work_mm: generated.required_first_move_position_work_mm,
                units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute,
                feed_mm_min: None,
                spindle_rpm: None,
                spindle_mode: NcSpindleMode::Stopped,
                tool_number: None,
            },
        )
        .unwrap();
        let issues = vec![
            VerificationIssue {
                severity: VerificationSeverity::Warning,
                code: cam_libre_visor::nc_verify::VerificationCode::RapidStockCandidate,
                source_line: 7,
                move_index: Some(1),
            },
            VerificationIssue {
                severity: VerificationSeverity::Warning,
                code: cam_libre_visor::nc_verify::VerificationCode::UnverifiedToolSelection,
                source_line: 3,
                move_index: None,
            },
        ];
        let targets = issue_motion_targets(&trace, &issues);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].unwrap().kind, NcMotionKind::LinearFeed);
        assert!(targets[1].is_none());
    }

    fn frame() -> WorkFrame {
        let bounds = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [30.0, 30.0, 30.0],
        };
        let stock = Stock {
            min: [0.0, 0.0, 0.0],
            size: [30.0, 30.0, 30.0],
            visible: true,
        };
        WorkFrame::new(&WorkOrigin::default(), &stock, &bounds).unwrap()
    }
    fn path() -> PlannedToolpath {
        PlannedToolpath {
            moves: vec![
                PlannedMove {
                    kind: PlannedMoveKind::RapidTraverse,
                    from: [0.0, 0.0, 25.4],
                    to: [25.4, 0.0, 25.4],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from: [25.4, 0.0, 25.4],
                    to: [25.4, 0.0, 12.7],
                    pass_index: 0,
                },
            ],
            ..Default::default()
        }
    }
    fn conditions() -> CuttingConditions {
        CuttingConditions {
            feed_mm_min: Some(254.0),
            spindle_rpm: Some(14000.0),
        }
    }

    #[test]
    fn model_to_work_happens_before_post_and_preserves_initial_precondition() {
        let frame = frame();
        let expected = frame.model_to_work(path().moves[0].from).unwrap();
        let nc = build_nc(
            &frame,
            &path(),
            conditions(),
            &PostProcessorProfile::grbl_3axis_mm(),
        )
        .unwrap();
        assert_eq!(nc.required_first_move_position_work_mm, expected);
        assert!(nc.text.contains("G90\n"));
        assert!(nc.text.contains("G0 "));
        assert!(nc.text.contains("G1 "));
        assert!(nc.requires_nc_interpretation);
    }

    #[test]
    fn independent_output_units_convert_feed_but_not_spindle() {
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.units = PostUnits::Inches;
        let nc = build_nc(&frame(), &path(), conditions(), &profile).unwrap();
        assert!(nc.text.contains("G20\n"));
        assert!(nc.text.contains("F10.000"));
        assert!(nc.text.contains("S14000\n"));
    }

    #[test]
    fn missing_conditions_cannot_be_silently_invented() {
        let error = build_nc(
            &frame(),
            &path(),
            CuttingConditions::default(),
            &PostProcessorProfile::grbl_3axis_mm(),
        )
        .unwrap_err();
        assert!(error.contains("F y S"));
    }

    #[test]
    fn filename_cannot_become_a_path_and_excerpt_contains_both_ends() {
        assert_eq!(
            suggested_filename("../../pieza * a", "nc"),
            "______pieza___a.nc"
        );
        let body = (0..100)
            .map(|i| format!("N{i} G1 X{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt = preview_excerpt(&body);
        assert!(excerpt.contains("N0 G1 X0"));
        assert!(excerpt.contains("N99 G1 X99"));
        assert!(excerpt.contains("se omiten"));
    }
}

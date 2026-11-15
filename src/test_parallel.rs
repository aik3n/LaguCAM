//! Acabado paralelo — TEST. Previsualizacion geometrica, sin G-code ni movimientos CNC.
//! El motor solo calcula alturas de contacto para una punta esferica; esta
//! capa decide donde muestrear, y dibuja los segmentos entre esos puntos.
use crate::{
    project::{
        Project,
        model::ModelReference,
        operations::{OperationKind, OperationStatus, ParallelTestParameters},
    },
    stl::Bounds,
    tools::{
        cam_adapter::{ResolvedCutter, ToolSignature, resolve_cutter, signature_current},
        model::ToolLibrary,
    },
};
use cam_libre_visor::cam_geometry::{
    contact_solver::{ContactQuery, ContactSolver, CutterGeometry},
    service::ContactEngineLimits,
};
use eframe::egui;
use glam::Vec3;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};

const MAX_POINTS: usize = 12_000;
const MAX_TRIANGLES: usize = 21_000_000;
const ENGINE_BYTES: usize = 700 * 1024 * 1024;
const CACHE_BYTES: usize = 32 * 1024 * 1024;

type Parameters = ParallelTestParameters;

#[derive(Clone, Debug)]
struct SamplingPlan {
    params: Parameters,
    bounds: Bounds,
    lines: usize,
    samples: usize,
}

fn count_positions(span: f64, step: f64) -> Result<usize, String> {
    if !span.is_finite() || span <= 0.0 || !step.is_finite() || step <= 0.0 {
        return Err("La extension y los pasos deben ser positivos y finitos".into());
    }
    let segments = (span / step).ceil().max(1.0);
    if !segments.is_finite() || segments > MAX_POINTS as f64 {
        return Err("Paso demasiado pequeno: demasiados puntos".into());
    }
    Ok(segments as usize + 1)
}

impl SamplingPlan {
    fn new(bounds: Bounds, params: Parameters) -> Result<Self, String> {
        if !bounds
            .min
            .iter()
            .chain(bounds.max.iter())
            .all(|x| x.is_finite())
            || !bounds.min.iter().zip(bounds.max).all(|(lo, hi)| hi >= *lo)
            || !params.diameter.is_finite()
            || params.diameter <= 0.0
        {
            return Err("Geometria o diametro invalidos".into());
        }
        params.validate()?;
        let mut bounds = bounds;
        if let Some(area) = params.area_xy {
            if area.x_min < bounds.min[0]
                || area.x_max > bounds.max[0]
                || area.y_min < bounds.min[1]
                || area.y_max > bounds.max[1]
            {
                return Err("La zona XY debe quedar dentro de los limites del STL".into());
            }
            bounds.min[0] = area.x_min;
            bounds.max[0] = area.x_max;
            bounds.min[1] = area.y_min;
            bounds.max[1] = area.y_max;
        }
        let along = if params.along_x { 0 } else { 1 };
        let across = 1 - along;
        let along_span = bounds.max[along] - bounds.min[along];
        let across_span = bounds.max[across] - bounds.min[across];
        let lines = count_positions(across_span, params.stepover)?;
        let samples = count_positions(along_span, params.longitudinal)?;
        if lines
            .checked_mul(samples)
            .is_none_or(|total| total > MAX_POINTS)
        {
            return Err(format!(
                "Demasiados puntos de consulta: el limite de esta prueba es {MAX_POINTS}"
            ));
        }
        let radius = params.diameter / 2.0;
        if !radius.is_finite() || radius <= 0.0 || !radius.powi(2).is_finite() {
            return Err("Diametro fuera del rango numerico del motor".into());
        }
        Ok(Self {
            params,
            bounds,
            lines,
            samples,
        })
    }

    fn total(&self) -> usize {
        self.lines * self.samples
    }

    /// El sobreespesor procede de la operacion, no de la biblioteca de herramientas.
    /// En TEST la escala es 1 unidad STL = 1 mm (misma hipotesis que G20).
    fn contact_query(&self, line: usize, sample: usize, cutter: CutterGeometry) -> ContactQuery {
        let [x, y] = self.xy(line, sample);
        ContactQuery {
            x_model: x,
            y_model: y,
            cutter,
            allowance_model: self.params.allowance_mm,
        }
    }

    fn xy(&self, line: usize, sample: usize) -> [f64; 2] {
        // Los extremos se incluyen expresamente; el ultimo tramo puede ser
        // mas corto, nunca mayor que el paso que introdujo el usuario.
        let along = if self.params.along_x { 0 } else { 1 };
        let across = 1 - along;
        let mut xy = [0.0; 2];
        let advance = |axis: usize, i: usize, n: usize, step: f64| {
            if i + 1 == n {
                self.bounds.max[axis]
            } else {
                self.bounds.min[axis]
                    + (i as f64 * step).min(self.bounds.max[axis] - self.bounds.min[axis])
            }
        };
        xy[along] = advance(along, sample, self.samples, self.params.longitudinal);
        xy[across] = advance(across, line, self.lines, self.params.stepover);
        xy
    }
}

#[derive(Debug)]
struct Outcome {
    lines: Vec<Vec<Option<[f64; 3]>>>,
    contacts: usize,
    lines_count: usize,
    samples_per_line: usize,
}

struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    total: usize,
    receiver: Receiver<Result<Outcome, String>>,
    signature: ToolSignature,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Job {
    fn start(
        path: PathBuf,
        expected_size: u64,
        expected_triangles: u32,
        plan: SamplingPlan,
        tool: ResolvedCutter,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let signature = tool.signature;
        let cutter = tool.cutter;
        let cancel = Arc::new(AtomicBool::new(false));
        let token = Arc::clone(&cancel);
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_progress = Arc::clone(&progress);
        let total = plan.total();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("lagucam-parallel-test".into())
            .spawn(move || {
                let result = compute(
                    &path,
                    expected_size,
                    expected_triangles,
                    &plan,
                    cutter,
                    &token,
                    &worker_progress,
                );
                let _ = sender.try_send(result);
                ctx.request_repaint();
            })
            .map_err(|e| format!("No se pudo iniciar el hilo de calculo: {e}"))?;
        Ok(Self {
            cancel,
            progress,
            total,
            receiver,
            signature,
        })
    }

    fn poll(&self) -> Option<Result<Outcome, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("El proceso termino sin entregar un resultado".into()))
            }
        }
    }
}

fn compute(
    path: &Path,
    expected_size: u64,
    expected_triangles: u32,
    plan: &SamplingPlan,
    cutter: CutterGeometry,
    cancelled: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<Outcome, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Calculo cancelado".into());
    }
    if expected_triangles == 0 || expected_triangles as usize > MAX_TRIANGLES {
        return Err("El STL supera el limite de triangulos de la prueba".into());
    }
    let size = fs::metadata(path)
        .map_err(|e| format!("No se puede abrir el STL: {e}"))?
        .len();
    if size != expected_size {
        return Err("El STL ha cambiado de tamano desde que se abrio".into());
    }
    let (mut engine, report) = ContactSolver::open_stl(
        path,
        ContactEngineLimits {
            max_triangles: MAX_TRIANGLES,
            max_accounted_bytes: ENGINE_BYTES,
            cache_bytes: CACHE_BYTES,
        },
        cancelled,
    )
    .map_err(|e| format!("Error preparando el motor geometrico: {e}"))?;
    if report.triangles != expected_triangles as usize {
        return Err("El numero de triangulos del STL no coincide con el modelo abierto".into());
    }
    let mut lines: Vec<Vec<Option<[f64; 3]>>> = Vec::with_capacity(plan.lines);
    let mut contacts = 0;
    for line in 0..plan.lines {
        let mut row = Vec::with_capacity(plan.samples);
        for sample in 0..plan.samples {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Calculo cancelado".into());
            }
            let request = plan.contact_query(line, sample, cutter);
            let [x, y] = [request.x_model, request.y_model];
            let hit = engine
                .solve(request, cancelled)
                .map_err(|e| format!("Error consultando contacto: {e}"))?;
            let point = hit.tip_z_model.map(|z| [x, y, z]);
            contacts += usize::from(point.is_some());
            row.push(point);
            progress.fetch_add(1, Ordering::Relaxed);
        }
        lines.push(row);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("Calculo cancelado".into());
    }
    if fs::metadata(path)
        .map_err(|e| format!("No se pudo verificar el STL: {e}"))?
        .len()
        != expected_size
    {
        return Err("El archivo STL cambio durante el calculo".into());
    }
    Ok(Outcome {
        lines,
        contacts,
        lines_count: plan.lines,
        samples_per_line: plan.samples,
    })
}

/// UI transitoria de operaciones: los parametros residen en Project,
/// los resultados y los hilos viven solamente en memoria.
// Estado del editor: pertenece a la interfaz y no al formato .lagucam.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum OperationEditorTab {
    #[default]
    General,
    Estrategia,
    Area,
}

#[derive(Clone)]
struct OperationEditorDraft {
    name: String,
    kind: OperationKind,
    tool_id: Option<String>,
    visible: bool,
}

impl OperationEditorDraft {
    fn from_operation(op: &crate::project::operations::Operation, visible: bool) -> Self {
        Self {
            name: op.name.clone(),
            kind: op.kind.clone(),
            tool_id: op.tool_id.clone(),
            visible,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 4096 {
            return Err("Escribe un nombre valido para la operacion".into());
        }
        if let OperationKind::ParallelTest(p) = &self.kind {
            p.validate()?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct TestParallelPanel {
    editing: Option<usize>,
    editor_draft: Option<OperationEditorDraft>,
    editor_tab: OperationEditorTab,
    editor_error: String,
    confirm_delete_operation: bool,
    active_job: Option<(usize, Job)>,
    cancelling: bool,
    results: HashMap<usize, Outcome>,
    result_signatures: HashMap<usize, ToolSignature>,
    hidden: HashSet<usize>,
    message: String,
}

impl TestParallelPanel {
    pub fn open_editor(&mut self, index: usize, project: &Project) {
        if let Some(op) = project.operations.items.get(index) {
            // Solo un editor: al editar otra operacion se descarta el borrador anterior.
            self.editing = Some(index);
            self.editor_draft = Some(OperationEditorDraft::from_operation(
                op,
                !self.hidden.contains(&index),
            ));
            self.editor_tab = OperationEditorTab::General;
            self.editor_error.clear();
            self.confirm_delete_operation = false;
        }
    }

    /// El resultado no es parte del proyecto: al borrar una operacion hay
    /// que olvidar su vista previa y desplazar los indices de las posteriores.
    /// Cualquier calculo en curso se cancela para evitar asociar su resultado
    /// a otra operacion despues del cambio de indices.
    pub fn operation_deleted(&mut self, deleted: usize) {
        self.active_job = None;
        self.cancelling = false;
        self.results = std::mem::take(&mut self.results)
            .into_iter()
            .filter_map(|(index, result)| index_after_delete(index, deleted).map(|i| (i, result)))
            .collect();
        self.result_signatures = std::mem::take(&mut self.result_signatures)
            .into_iter()
            .filter_map(|(index, signature)| {
                index_after_delete(index, deleted).map(|i| (i, signature))
            })
            .collect();
        self.hidden = std::mem::take(&mut self.hidden)
            .into_iter()
            .filter_map(|index| index_after_delete(index, deleted))
            .collect();
        let previous_editor = self.editing;
        self.editing = self
            .editing
            .and_then(|index| index_after_delete(index, deleted));
        if previous_editor != self.editing {
            // Cerrar el editor si la operacion borrada estaba abierta; si
            // era una anterior, conservar el borrador de la que sigue abierta.
            if self.editing.is_none() {
                self.editor_draft = None;
                self.editor_error.clear();
            }
        }
        self.message = "Operación eliminada del proyecto".into();
    }

    pub fn clear_source(&mut self) {
        // Drop del Job solicita cancelacion; el receptor se descarta.
        self.active_job = None;
        self.cancelling = false;
        self.results.clear();
        self.result_signatures.clear();
        self.hidden.clear();
        self.editing = None;
        self.editor_draft = None;
        self.editor_tab = OperationEditorTab::General;
        self.editor_error.clear();
        self.confirm_delete_operation = false;
        self.message.clear();
    }

    /// Cada resultado procede de una identidad y huella fisica concretas.
    /// El cambio de herramienta o su eliminacion invalida la muestra.
    pub fn sync_tools(&mut self, project: &mut Project, library: &ToolLibrary) {
        let current = |index: usize, signature: &ToolSignature| {
            project
                .operations
                .items
                .get(index)
                .is_some_and(|op| signature_current(library, op.tool_id.as_deref(), signature))
        };
        let stale: Vec<usize> = self
            .result_signatures
            .iter()
            .filter_map(|(&i, signature)| (!current(i, signature)).then_some(i))
            .collect();
        let cancel_running = self
            .active_job
            .as_ref()
            .is_some_and(|(i, job)| !current(*i, &job.signature));
        for index in stale {
            self.results.remove(&index);
            self.result_signatures.remove(&index);
            if let Some(op) = project.operations.items.get_mut(index) {
                op.status = OperationStatus::NotCalculated;
            }
        }
        if cancel_running {
            let (index, _) = self.active_job.take().expect("Trabajo activo invalidado");
            self.cancelling = false;
            self.results.remove(&index);
            self.result_signatures.remove(&index);
            if let Some(op) = project.operations.items.get_mut(index) {
                op.status = OperationStatus::NotCalculated;
            }
            self.message = "Herramienta modificada o eliminada: calculo invalidado".into();
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context, project: &mut Project) {
        let completed = self.active_job.as_ref().and_then(|(_, job)| job.poll());
        if let Some(result) = completed {
            let (index, job) = self.active_job.take().expect("Existe trabajo terminado");
            let was_cancelling = std::mem::take(&mut self.cancelling);
            if was_cancelling {
                self.results.remove(&index);
                if let Some(op) = project.operations.items.get_mut(index) {
                    op.status = OperationStatus::NotCalculated;
                }
                self.message = "Calculo cancelado.".into();
                return;
            }
            match result {
                Ok(outcome) => {
                    self.message = format!(
                        "{} pasadas, {} puntos por pasada, {} contactos.",
                        outcome.lines_count, outcome.samples_per_line, outcome.contacts
                    );
                    self.results.insert(index, outcome);
                    self.result_signatures.insert(index, job.signature.clone());
                    self.hidden.remove(&index);
                    if let Some(op) = project.operations.items.get_mut(index) {
                        op.status = OperationStatus::Ready;
                    }
                }
                Err(error) => {
                    self.results.remove(&index);
                    self.result_signatures.remove(&index);
                    if let Some(op) = project.operations.items.get_mut(index) {
                        op.status = OperationStatus::Error;
                    }
                    self.message = format!("ERROR: {error}");
                }
            }
        } else if self.active_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    pub fn show_editor(
        &mut self,
        ctx: &egui::Context,
        project: &mut Project,
        stl_path: Option<&Path>,
        tool_library: &ToolLibrary,
        available: bool,
    ) {
        let Some(index) = self.editing else {
            return;
        };
        let Some(op) = project.operations.items.get(index) else {
            self.editing = None;
            self.editor_draft = None;
            return;
        };
        let Some(mut draft) = self.editor_draft.clone() else {
            self.editing = None;
            return;
        };
        let title = format!("Operacion · {}", op.name);
        let mut tab = self.editor_tab;
        let model_bounds = project.model.as_ref().map(|m| m.bounds);
        let running = self.active_job.is_some();
        let running_this = self.active_job.as_ref().is_some_and(|(i, _)| *i == index);
        let progress = self
            .active_job
            .as_ref()
            .filter(|(i, _)| *i == index)
            .map(|(_, j)| (j.progress.load(Ordering::Relaxed), j.total));
        let has_result = self.results.contains_key(&index);
        let mut accept = false;
        let mut generate = false;
        let mut cancel_edit = false;
        let mut cancel_job = false;
        let mut delete_now = false;
        let mut confirm_delete = self.confirm_delete_operation;
        let mut window_open = true;

        egui::Window::new(title)
            .open(&mut window_open)
            .resizable(true)
            .default_width(610.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(tab == OperationEditorTab::General, "General").clicked() {
                        tab = OperationEditorTab::General;
                    }
                    if ui.selectable_label(tab == OperationEditorTab::Estrategia, "Estrategia").clicked() {
                        tab = OperationEditorTab::Estrategia;
                    }
                    if ui.selectable_label(tab == OperationEditorTab::Area, "Area").clicked() {
                        tab = OperationEditorTab::Area;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        ui.set_min_height(200.0);
                        match tab {
                            OperationEditorTab::General => {
                                ui.heading("Datos generales");
                                ui.add_enabled_ui(!running, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Nombre:");
                                        ui.text_edit_singleline(&mut draft.name);
                                    });
                                });
                                ui.separator();
                                ui.add_enabled_ui(!running, |ui| {
                                    let selected = match draft.tool_id.as_deref() {
                                        None => "Sin seleccionar".to_owned(),
                                        Some(id) => tool_library.get(id)
                                            .map(|tool| tool.name.clone())
                                            .unwrap_or_else(|| "Herramienta no disponible".to_owned()),
                                    };
                                    egui::ComboBox::from_label("Herramienta de biblioteca")
                                        .selected_text(selected)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut draft.tool_id, None, "Sin seleccionar");
                                            for tool in &tool_library.tools {
                                                ui.selectable_value(
                                                    &mut draft.tool_id,
                                                    Some(tool.id.clone()),
                                                    format!("{} - {} Ø{:.3} mm", tool.name,
                                                        tool.geometry.kind().label(),
                                                        tool.geometry.diameter_mm()),
                                                );
                                            }
                                        });
                                });
                                if let Some(id) = draft.tool_id.as_deref() {
                                    if let Some(tool) = tool_library.get(id) {
                                        ui.small(format!("{} · Diametro Ø{:.3} mm (biblioteca)",
                                            tool.geometry.kind().label(), tool.geometry.diameter_mm()));
                                        if !matches!(&tool.geometry, crate::tools::model::ToolGeometry::BallNose { .. }) {
                                            ui.small("Este tipo aun no dispone de solucion de contacto G19.");
                                        }
                                    } else {
                                        ui.colored_label(egui::Color32::YELLOW, "La fresa asignada no existe en la biblioteca.");
                                    }
                                }
                                ui.small("TEST G20: se supone que las unidades del STL son milimetros.");
                                ui.small("La operacion puede guardarse sin una estrategia seleccionada.");
                            }
                            OperationEditorTab::Estrategia => {
                                ui.heading("Estrategia de mecanizado");
                                let was_parallel = matches!(&draft.kind, OperationKind::ParallelTest(_));
                                let mut parallel = was_parallel;
                                ui.add_enabled_ui(!running, |ui| {
                                    egui::ComboBox::from_label("Estrategia")
                                        .selected_text(if was_parallel {
                                            "Acabado paralelo - TEST"
                                        } else {
                                            "Sin seleccionar"
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut parallel, false, "Sin seleccionar");
                                            ui.selectable_value(&mut parallel, true, "Acabado paralelo - TEST");
                                        });
                                });
                                if parallel != was_parallel {
                                    draft.kind = if parallel {
                                        OperationKind::ParallelTest(Parameters::default())
                                    } else {
                                        OperationKind::Placeholder
                                    };
                                }
                                if let OperationKind::ParallelTest(ref mut p) = draft.kind {
                                    ui.separator();
                                    ui.add_enabled_ui(!running, |ui| {
                                        ui.small("Diametro de contacto: definido por la herramienta seleccionada en General.");
                                        ui.horizontal(|ui| {
                                            ui.label("Paso entre pasadas:");
                                            ui.add(egui::DragValue::new(&mut p.stepover)
                                                .speed(0.5).range(0.01..=1000.0));
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Paso longitudinal:");
                                            ui.add(egui::DragValue::new(&mut p.longitudinal)
                                                .speed(0.1).range(0.01..=1000.0));
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Sobreespesor:");
                                            ui.add(egui::DragValue::new(&mut p.allowance_mm)
                                                .speed(0.1).range(0.0..=1_000_000.0).suffix(" mm"));
                                        });
                                        ui.small("Separacion uniforme respecto al modelo nominal; no se acumula entre operaciones.");
                                        ui.horizontal(|ui| {
                                            ui.label("Direccion:");
                                            ui.radio_value(&mut p.along_x, true, "Eje X");
                                            ui.radio_value(&mut p.along_x, false, "Eje Y");
                                        });
                                    });
                                } else {
                                    ui.small("Selecciona una estrategia para acceder a sus parametros.");
                                }
                            }
                            OperationEditorTab::Area => {
                                ui.heading("Area de mecanizado");
                                if let OperationKind::ParallelTest(ref mut p) = draft.kind {
                                    ui.add_enabled_ui(!running, |ui| {
                                        let mut limited = p.area_xy.is_some();
                                        ui.checkbox(&mut limited, "Limitar zona XY");
                                        if limited != p.area_xy.is_some() {
                                            p.area_xy = if limited {
                                                model_bounds.map(|bounds| {
                                                    crate::project::operations::TestAreaXY {
                                                        x_min: bounds.min[0],
                                                        x_max: bounds.max[0],
                                                        y_min: bounds.min[1],
                                                        y_max: bounds.max[1],
                                                    }
                                                })
                                            } else {
                                                None
                                            };
                                        }
                                        if let Some(area) = p.area_xy.as_mut() {
                                            ui.small("Limites en coordenadas originales del STL.");
                                            ui.horizontal(|ui| {
                                                ui.label("X min");
                                                ui.add(egui::DragValue::new(&mut area.x_min).speed(0.1));
                                                ui.label("X max");
                                                ui.add(egui::DragValue::new(&mut area.x_max).speed(0.1));
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Y min");
                                                ui.add(egui::DragValue::new(&mut area.y_min).speed(0.1));
                                                ui.label("Y max");
                                                ui.add(egui::DragValue::new(&mut area.y_max).speed(0.1));
                                            });
                                        } else {
                                            ui.small("Se utilizara el rectangulo completo del modelo STL.");
                                        }
                                    });
                                } else {
                                    ui.label("Selecciona primero una estrategia en la pestana Estrategia.");
                                }
                            }
                        }
                    });
                ui.separator();
                let validation = draft.validate();
                if let Err(error) = &validation {
                    ui.colored_label(egui::Color32::YELLOW, error.as_str());
                }
                let plan = if let (Some(bounds), OperationKind::ParallelTest(p)) = (model_bounds, &draft.kind) {
                    Some(resolve_cutter(tool_library, draft.tool_id.as_deref(), 1.0)
                        .and_then(|resolved| {
                            resolved.ensure_supported()?;
                            let mut effective = *p;
                            // El diametro serializado es legado: no es fuente CAM.
                            effective.diameter = resolved.diameter_model();
                            SamplingPlan::new(bounds, effective)
                        }))
                } else {
                    None
                };
                if validation.is_ok() {
                    match &plan {
                        Some(Ok(p)) => {
                            ui.small(format!("{} pasadas x {} puntos = {} consultas", p.lines, p.samples, p.total()));
                        }
                        Some(Err(error)) => {
                            ui.colored_label(egui::Color32::YELLOW, error.as_str());
                        }
                        None => {
                            ui.small("Una operacion sin estrategia no puede generar.");
                        }
                    }
                }
                if !self.editor_error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.editor_error);
                }
                if let Some((done, total)) = progress {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Calculando: {done} / {total}"));
                    });
                }
                if self.cancelling && running_this {
                    ui.small("Cancelando calculo...");
                }
                if !self.message.is_empty() {
                    ui.small(&self.message);
                }
                if has_result {
                    ui.checkbox(&mut draft.visible, "Mostrar resultado 3D");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let can_accept = !running && validation.is_ok();
                    if ui.add_enabled(can_accept, egui::Button::new("Aceptar")).clicked() {
                        accept = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancel_edit = true;
                    }
                    let can_generate = !running && available && stl_path.is_some()
                        && validation.is_ok() && matches!(plan, Some(Ok(_)));
                    if ui.add_enabled(can_generate, egui::Button::new("Generar")).clicked() {
                        generate = true;
                    }
                    if ui.add_enabled(running_this && !self.cancelling,
                        egui::Button::new("Cancelar calculo")).clicked() {
                        cancel_job = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!running, egui::Button::new("Eliminar")).clicked() {
                            confirm_delete = true;
                        }
                    });
                });
                if confirm_delete && !running {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::LIGHT_RED, "¿Eliminar definitivamente esta operación?");
                        if ui.button("Sí, eliminar").clicked() {
                            delete_now = true;
                        }
                        if ui.button("No").clicked() {
                            confirm_delete = false;
                        }
                    });
                }
                ui.small("TEST: maximo de 12.000 consultas. No genera G-code.");
            });

        if delete_now {
            // Se elimina la operacion seleccionada, no los cambios sin confirmar.
            // No se permite borrar mientras un calculo esta en curso.
            self.operation_deleted(index);
            project.operations.items.remove(index);
            project.selection = match project.selection {
                crate::project::selection::Selection::Operation(i) if i == index => {
                    crate::project::selection::Selection::Operations
                }
                crate::project::selection::Selection::Operation(i) if i > index => {
                    crate::project::selection::Selection::Operation(i - 1)
                }
                other => other,
            };
            self.confirm_delete_operation = false;
            ctx.request_repaint();
            return;
        }

        if cancel_job {
            if let Some((_, job)) = &self.active_job {
                job.cancel.store(true, Ordering::Relaxed);
                self.cancelling = true;
                self.results.remove(&index);
                self.message = "Cancelacion solicitada...".into();
            }
        }
        if accept || generate {
            if draft.visible {
                self.hidden.remove(&index);
            } else {
                self.hidden.insert(index);
            }
            // Aplicar una unica vez: el proyecto solo cambia al aceptar o generar.
            if let Some(op) = project.operations.items.get_mut(index) {
                let modified =
                    op.name != draft.name || op.kind != draft.kind || op.tool_id != draft.tool_id;
                if modified {
                    op.name = draft.name.clone();
                    op.kind = draft.kind.clone();
                    op.tool_id = draft.tool_id.clone();
                    op.status = OperationStatus::NotCalculated;
                    self.results.remove(&index);
                    self.result_signatures.remove(&index);
                    self.message = "Configuracion cambiada. Pendiente de recalcular.".into();
                }
            }
            self.editor_error.clear();
        }
        if generate {
            confirm_delete = false;
            let params = project.operations.items.get(index).and_then(|op| {
                if let OperationKind::ParallelTest(p) = &op.kind {
                    Some(*p)
                } else {
                    None
                }
            });
            if let (Some(model), Some(path), Some(params)) =
                (project.model.as_ref(), stl_path, params)
            {
                let prepared = resolve_cutter(
                    tool_library,
                    project.operations.items[index].tool_id.as_deref(),
                    1.0,
                )
                .and_then(|tool| {
                    tool.ensure_supported()?;
                    let mut effective = params;
                    effective.diameter = tool.diameter_model();
                    SamplingPlan::new(model.bounds, effective).map(|plan| (plan, tool))
                });
                match prepared {
                    Ok((plan, tool)) => match Job::start(
                        path.to_path_buf(),
                        model.size_bytes,
                        model.triangles,
                        plan,
                        tool,
                        ctx.clone(),
                    ) {
                        Ok(job) => {
                            self.active_job = Some((index, job));
                            self.cancelling = false;
                            self.results.remove(&index);
                            self.result_signatures.remove(&index);
                            self.hidden.remove(&index);
                            self.message = "Preparando motor y calculando...".into();
                            if let Some(op) = project.operations.items.get_mut(index) {
                                op.status = OperationStatus::NotCalculated;
                            }
                        }
                        Err(error) => self.editor_error = format!("ERROR: {error}"),
                    },
                    Err(error) => self.editor_error = format!("ERROR: {error}"),
                }
            }
        }
        if !window_open || cancel_edit || accept {
            self.editing = None;
            self.confirm_delete_operation = false;
            self.editor_draft = None;
            self.editor_error.clear();
        } else {
            self.editor_draft = Some(draft);
            self.editor_tab = tab;
            self.confirm_delete_operation = confirm_delete;
        }
    }

    pub fn paint(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        model: &ModelReference,
        yaw: f32,
        pitch: f32,
        zoom: f32,
        pan: Vec3,
    ) {
        if self.results.is_empty() {
            return;
        }
        let mvp = crate::navigation::view_projection(
            yaw,
            pitch,
            zoom,
            rect.width() / rect.height().max(1.0),
            pan,
        );
        let painter = ui.painter().with_clip_rect(rect);
        let project = |xyz: [f64; 3]| -> Option<egui::Pos2> {
            let pos = model.visual_point(xyz)?;
            let clip = mvp * pos.extend(1.0);
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
        let mut indices: Vec<_> = self.results.keys().copied().collect();
        indices.sort_unstable();
        for index in indices {
            if self.hidden.contains(&index) {
                continue;
            }
            let result = &self.results[&index];
            for (line_index, line) in result.lines.iter().enumerate() {
                let color = if (index + line_index) % 2 == 0 {
                    egui::Color32::from_rgb(35, 230, 235)
                } else {
                    egui::Color32::from_rgb(255, 195, 65)
                };
                let mut previous = None;
                for item in line {
                    let next = (*item).and_then(&project);
                    if let (Some(a), Some(b)) = (previous, next) {
                        painter.line_segment([a, b], egui::Stroke::new(1.8, color));
                    }
                    previous = next;
                }
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(12.0, 64.0),
            egui::Align2::LEFT_TOP,
            format!(
                "PARALELO TEST · {} operaciones calculadas · sin G-code",
                self.results.len()
            ),
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            min: [0.0, 10.0, -4.0],
            max: [10.0, 16.0, 9.0],
        }
    }

    #[test]
    fn endpoints_and_short_last_step_are_included() {
        let plan = SamplingPlan::new(
            bounds(),
            Parameters {
                diameter: 2.0,
                stepover: 4.0,
                longitudinal: 3.0,
                along_x: true,
                allowance_mm: 0.0,
                area_xy: None,
            },
        )
        .unwrap();
        assert_eq!((plan.lines, plan.samples), (3, 5));
        assert_eq!(plan.xy(0, 0), [0.0, 10.0]);
        assert_eq!(plan.xy(1, 3), [9.0, 14.0]);
        assert_eq!(plan.xy(2, 4), [10.0, 16.0]);
    }

    #[test]
    fn y_direction_interchanges_sample_and_pass_axes() {
        let plan = SamplingPlan::new(
            bounds(),
            Parameters {
                diameter: 2.0,
                stepover: 4.0,
                longitudinal: 3.0,
                along_x: false,
                allowance_mm: 0.0,
                area_xy: None,
            },
        )
        .unwrap();
        assert_eq!((plan.lines, plan.samples), (4, 3));
        assert_eq!(plan.xy(0, 0), [0.0, 10.0]);
        assert_eq!(plan.xy(3, 2), [10.0, 16.0]);
    }

    #[test]
    fn invalid_or_excessive_parameters_are_rejected() {
        let mut params = Parameters::default();
        params.longitudinal = 0.00001;
        assert!(SamplingPlan::new(bounds(), params).is_err());
        params.longitudinal = 2.0;
        params.stepover = f64::NAN;
        assert!(SamplingPlan::new(bounds(), params).is_err());
        params.stepover = 2.0;
        params.diameter = -1.0;
        assert!(SamplingPlan::new(bounds(), params).is_err());
        params.diameter = 4.0;
        assert!(SamplingPlan::new(bounds(), params).is_ok());
    }

    #[test]
    fn cancellation_stops_before_opening_source() {
        let cancel = AtomicBool::new(true);
        let plan = SamplingPlan::new(bounds(), Parameters::default()).unwrap();
        assert!(
            compute(
                Path::new("no_existe_parallel_test.stl"),
                134,
                1,
                &plan,
                CutterGeometry::BallNose {
                    diameter_model: 4.0
                },
                &cancel,
                &AtomicUsize::new(0)
            )
            .unwrap_err()
            .contains("cancelado")
        );
    }
}

#[cfg(test)]
mod test_area_xy_sampling {
    use super::*;

    #[test]
    fn limited_area_uses_exact_new_boundaries() {
        let mut p = Parameters::default();
        p.area_xy = Some(TestAreaXY {
            x_min: 2.0,
            x_max: 7.0,
            y_min: 12.0,
            y_max: 15.0,
        });
        p.stepover = 4.0;
        p.longitudinal = 3.0;
        let plan = SamplingPlan::new(
            Bounds {
                min: [0.0, 10.0, -4.0],
                max: [10.0, 16.0, 9.0],
            },
            p,
        )
        .unwrap();
        assert_eq!((plan.lines, plan.samples), (2, 3));
        assert_eq!(plan.xy(0, 0), [2.0, 12.0]);
        assert_eq!(plan.xy(1, 2), [7.0, 15.0]);
    }

    #[test]
    fn area_outside_stl_is_not_silently_clipped() {
        let mut p = Parameters::default();
        p.area_xy = Some(TestAreaXY {
            x_min: -1.0,
            x_max: 7.0,
            y_min: 12.0,
            y_max: 15.0,
        });
        assert!(
            SamplingPlan::new(
                Bounds {
                    min: [0.0, 10.0, -4.0],
                    max: [10.0, 16.0, 9.0]
                },
                p,
            )
            .unwrap_err()
            .contains("limites")
        );
    }
}

#[cfg(test)]
mod operation_editor_tabs_tests {
    use super::*;

    #[test]
    fn opening_an_empty_operation_preserves_it_until_commit() {
        let mut project = Project::default();
        let index = project.operations.add_new().unwrap();
        let original = project.operations.items[index].name.clone();
        let mut panel = TestParallelPanel::default();
        panel.open_editor(index, &project);
        let draft = panel.editor_draft.as_mut().unwrap();
        draft.name = "Mecanizado personalizado".into();
        draft.kind = OperationKind::ParallelTest(Parameters::default());
        assert_eq!(project.operations.items[index].name, original);
        assert!(matches!(
            &project.operations.items[index].kind,
            OperationKind::Placeholder
        ));
    }

    #[test]
    fn validates_name_and_test_strategy_parameters() {
        let mut draft = OperationEditorDraft {
            name: " ".into(),
            kind: OperationKind::Placeholder,
            tool_id: None,
            visible: true,
        };
        assert!(draft.validate().is_err());
        draft.name = "Operacion A".into();
        assert!(draft.validate().is_ok());
        let mut p = Parameters::default();
        p.stepover = -1.0;
        draft.kind = OperationKind::ParallelTest(p);
        assert!(draft.validate().is_err());
    }
}

/// Tras eliminar un elemento, una referencia a su propio indice deja de existir.
fn index_after_delete(index: usize, deleted: usize) -> Option<usize> {
    if index == deleted {
        None
    } else {
        Some(index - usize::from(index > deleted))
    }
}

#[cfg(test)]
mod deletion_index_tests {
    use super::*;

    #[test]
    fn deletion_removes_the_entry_and_reindexes_later_operations() {
        assert_eq!(index_after_delete(0, 1), Some(0));
        assert_eq!(index_after_delete(1, 1), None);
        assert_eq!(index_after_delete(2, 1), Some(1));
        assert_eq!(index_after_delete(3, 1), Some(2));
        let mut panel = TestParallelPanel::default();
        panel.hidden.extend([0, 2, 3]);
        panel.result_signatures.insert(
            2,
            ToolSignature {
                tool_id: "fresa".to_owned(),
                geometry_hash: [0; 32],
            },
        );
        panel.editing = Some(3);
        panel.operation_deleted(1);
        assert_eq!(panel.editing, Some(2));
        assert_eq!(panel.hidden.len(), 3);
        assert!(panel.hidden.contains(&0));
        assert!(panel.hidden.contains(&1));
        assert!(panel.hidden.contains(&2));
        assert!(panel.result_signatures.contains_key(&1));
        assert!(!panel.result_signatures.contains_key(&2));
    }
}

#[cfg(test)]
use crate::project::operations::TestAreaXY;

#[cfg(test)]
mod g20_library_sync_tests {
    use super::*;
    use crate::tools::model::{Tool, ToolGeometry, ToolKind};

    #[test]
    fn changing_geometry_or_deleting_tool_invalidates_cached_preview() {
        let mut lib = ToolLibrary::default();
        let mut tool = Tool::blank(ToolKind::BallNose);
        tool.name = "Esferica".into();
        tool.geometry = ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        };
        tool.cutting_length_mm = 15.0;
        tool.shank_diameter_mm = 4.0;
        tool.total_length_mm = 30.0;
        let id = lib.add(tool).unwrap();
        let signature = resolve_cutter(&lib, Some(&id), 1.0).unwrap().signature;
        let mut project = Project::default();
        let index = project.operations.add_new().unwrap();
        project.operations.items[index].tool_id = Some(id.clone());
        project.operations.items[index].status = OperationStatus::Ready;
        let mut panel = TestParallelPanel::default();
        panel.results.insert(
            index,
            Outcome {
                lines: vec![],
                contacts: 0,
                lines_count: 0,
                samples_per_line: 0,
            },
        );
        panel.result_signatures.insert(index, signature);
        panel.sync_tools(&mut project, &lib);
        assert!(panel.results.contains_key(&index));

        let mut updated = lib.get(&id).unwrap().clone();
        updated.name = "Nuevo nombre".into();
        lib.replace(updated).unwrap();
        panel.sync_tools(&mut project, &lib);
        assert!(panel.results.contains_key(&index));

        let mut updated = lib.get(&id).unwrap().clone();
        updated.geometry = ToolGeometry::BallNose {
            cutting_diameter_mm: 6.0,
        };
        lib.replace(updated).unwrap();
        panel.sync_tools(&mut project, &lib);
        assert!(!panel.results.contains_key(&index));
        assert!(matches!(
            project.operations.items[index].status,
            OperationStatus::NotCalculated
        ));
    }
}

#[cfg(test)]
mod g23_operation_to_solver_tests {
    use super::*;

    #[test]
    fn contact_request_preserves_allowance_and_real_cutter_geometry() {
        let bounds = Bounds {
            min: [5.0, 5.0, 0.0],
            max: [10.0, 10.0, 10.0],
        };
        let mut params = Parameters::default();
        params.allowance_mm = 2.5;
        let plan = SamplingPlan::new(bounds, params).unwrap();
        let cutter = CutterGeometry::Flat {
            diameter_model: 12.0,
        };
        let query = plan.contact_query(0, 0, cutter);
        assert_eq!((query.x_model, query.y_model), (5.0, 5.0));
        assert_eq!(query.cutter, cutter);
        assert_eq!(query.allowance_model, 2.5);
    }

    #[cfg(windows)]
    #[test]
    fn parallel_test_passes_positive_allowance_to_real_solver_for_both_cutters() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        // Plano horizontal de una triangulacion STL sintética: z=5.
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 12]);
        for p in [[0.0f32, 0.0, 5.0], [100.0, 0.0, 5.0], [0.0, 100.0, 5.0]] {
            for coordinate in p {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
        let file = std::env::temp_dir().join(format!("lagucam-g23-{}.stl", uuid::Uuid::new_v4()));
        std::fs::write(&file, &bytes).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(file.clone());
        let bounds = Bounds {
            min: [5.0, 5.0, 5.0],
            max: [10.0, 10.0, 5.0],
        };
        let cancel = AtomicBool::new(false);
        for cutter in [
            CutterGeometry::Flat {
                diameter_model: 4.0,
            },
            CutterGeometry::BallNose {
                diameter_model: 4.0,
            },
        ] {
            let mut params = Parameters::default();
            params.diameter = 4.0;
            params.stepover = 5.0;
            params.longitudinal = 5.0;
            params.allowance_mm = 0.0;
            let nominal_plan = SamplingPlan::new(bounds, params).unwrap();
            let original = compute(
                &file,
                bytes.len() as u64,
                1,
                &nominal_plan,
                cutter,
                &cancel,
                &AtomicUsize::new(0),
            )
            .unwrap();
            params.allowance_mm = 2.0;
            let roughing_plan = SamplingPlan::new(bounds, params).unwrap();
            let roughing = compute(
                &file,
                bytes.len() as u64,
                1,
                &roughing_plan,
                cutter,
                &cancel,
                &AtomicUsize::new(0),
            )
            .unwrap();
            let nominal_z = original.lines[0][0].unwrap()[2];
            let roughing_z = roughing.lines[0][0].unwrap()[2];
            assert!((nominal_z - 5.0).abs() < 1.0e-10, "{cutter:?}: {nominal_z}");
            assert!(
                (roughing_z - 7.0).abs() < 1.0e-10,
                "{cutter:?}: {roughing_z}"
            );
        }
    }
}

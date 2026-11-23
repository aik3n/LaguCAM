//! Acabado paralelo — TEST. Previsualizacion geometrica, sin G-code ni movimientos CNC.
//! El motor solo calcula alturas de contacto para una punta esferica; esta
//! capa decide donde muestrear, y dibuja los segmentos entre esos puntos.
use crate::tools::cam_adapter::ToolSignature;
use crate::{
    project::{
        Project,
        cam_coordinates::WorkFrame,
        cam_provenance::CamProvenance,
        model::ModelReference,
        operations::{OperationKind, OperationStatus, ParallelTestParameters},
    },
    stl::Bounds,
    tools::{
        cam_adapter::{ResolvedCutter, resolve_cutter},
        model::ToolLibrary,
    },
};
use cam_libre_visor::cam_contract::{CamOperationView, CuttingConditions};
use cam_libre_visor::cam_geometry::{
    contact_solver::{ContactQuery, ContactSolver, CutterGeometry},
    service::ContactEngineLimits,
    stl::GeometryAffine,
};
use cam_libre_visor::toolpath::{RasterPattern, Toolpath};
use cam_libre_visor::toolpath_links::{PlannedMoveKind, PlannedToolpath};
use cam_libre_visor::toolpath_validation::{Severity, ValidationReport, validate_planned_toolpath};
use eframe::egui;
use glam::Vec3;
use std::{
    collections::HashMap,
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

// Presupuesto superior a la cuota experimental de 12.000 consultas.
// Protege la memoria del raster; el post NC conserva sus propios limites.
const MAX_POINTS: usize = 1_000_000;
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
    reference_z: f64,
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
                "El muestreo supera el limite tecnico actual ({MAX_POINTS} consultas)"
            ));
        }
        let radius = params.diameter / 2.0;
        if !radius.is_finite() || radius <= 0.0 || !radius.powi(2).is_finite() {
            return Err("Diametro fuera del rango numerico del motor".into());
        }
        Ok(Self {
            params,
            reference_z: bounds.max[2],
            bounds,
            lines,
            samples,
        })
    }

    /// Cota maxima del STL y del bruto. Fallar ante bruto invalido, no ignorarlo.
    fn with_stock(mut self, stock: Option<&crate::project::stock::Stock>) -> Result<Self, String> {
        if let Some(stock) = stock {
            // max() puede ser finito aunque min + size redondee a min en f64.
            // No aceptar un bruto que pierde una dimension en esta escala.
            stock
                .validate()
                .map_err(|reason| format!("Bruto invalido: {reason}"))?;
            let maximum = stock.max().ok_or("Limites del bruto fuera de rango")?;
            if maximum.iter().zip(stock.min).any(|(&hi, lo)| hi <= lo) {
                return Err("Dimensiones del bruto no representables en f64".into());
            }
            self.reference_z = self.reference_z.max(maximum[2]);
        }
        Ok(self)
    }

    fn total(&self) -> usize {
        self.lines * self.samples
    }

    /// El sobreespesor procede de la operacion, no de la biblioteca de herramientas.
    /// En TEST, el muestreo y las herramientas usan el modelo fisico en mm.
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
    toolpath: Toolpath,
    linked: PlannedToolpath,
    // G25.4b: informe de la MISMA secuencia calculada; no se persiste en .lagucam.
    validation: ValidationReport,
    conditions: CuttingConditions,
    contacts: usize,
    lines_count: usize,
    samples_per_line: usize,
}

/// Ready indica un calculo estructuralmente valido con corte, NO seguridad CNC.
/// Una operacion sin corte conserva el diagnostico, pero no se anuncia lista.
fn validation_operation_status(report: &ValidationReport) -> OperationStatus {
    if !report.is_structurally_valid() {
        OperationStatus::Error
    } else if report.cutting_moves == 0 {
        OperationStatus::NotCalculated
    } else {
        OperationStatus::Ready
    }
}

fn validation_label(report: &ValidationReport) -> &'static str {
    if !report.is_structurally_valid() {
        "ERROR estructural"
    } else if report.cutting_moves == 0 {
        "SIN MECANIZADO (aviso)"
    } else if report.issues.is_empty() {
        "CORRECTA estructuralmente"
    } else {
        "CORRECTA con avisos"
    }
}

struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    total: usize,
    receiver: Receiver<Result<Outcome, String>>,
    provenance: CamProvenance,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Job {
    #[expect(
        clippy::too_many_arguments,
        reason = "Contrato existente del trabajo CAM; refactorizacion fuera del alcance UI"
    )]
    fn start(
        path: PathBuf,
        expected_size: u64,
        expected_triangles: u32,
        plan: SamplingPlan,
        tool: ResolvedCutter,
        provenance: CamProvenance,
        affine: Option<GeometryAffine>,
        ctx: egui::Context,
    ) -> Result<Self, String> {
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
                    affine,
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
            provenance,
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

#[expect(
    clippy::too_many_arguments,
    reason = "Contrato existente del nucleo CAM; no alterar calculo en una limpieza UI"
)]
fn compute(
    path: &Path,
    expected_size: u64,
    expected_triangles: u32,
    plan: &SamplingPlan,
    cutter: CutterGeometry,
    affine: Option<GeometryAffine>,
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
    let limits = ContactEngineLimits {
        max_triangles: MAX_TRIANGLES,
        max_accounted_bytes: ENGINE_BYTES,
        cache_bytes: CACHE_BYTES,
    };
    let (mut engine, report) = match affine {
        Some(matrix) => ContactSolver::open_stl_transformed(path, limits, cancelled, matrix),
        None => ContactSolver::open_stl(path, limits, cancelled),
    }
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
    // G25.1: los huecos de contacto separan segmentos; nunca se inventan enlaces.
    let toolpath =
        Toolpath::from_contact_grid_with_pattern(lines, MAX_POINTS, plan.params.pass_pattern)
            .map_err(|error| format!("Error construyendo trayectoria CAM: {error}"))?;
    debug_assert_eq!(contacts, toolpath.contact_points);
    // G25.3: solo previsualizacion; ningun G0 real ni certificacion de colision.
    let linked = PlannedToolpath::from_toolpath(
        &toolpath,
        plan.reference_z,
        plan.params.approach_mm,
        plan.params.clearance_mm,
    )
    .map_err(|e| format!("Error planificando movimientos propuestos: {e}"))?;
    // La validacion es inmutable y trabaja con la secuencia final de movimientos.
    let validation = validate_planned_toolpath(&linked);
    let conditions = plan.params.cutting_conditions();
    // La misma trayectoria validada y sus datos F/S forman la vista CAM comun.
    // La vista NO cambia ni recalcula movimientos; la salida sigue en mm/modelo.
    CamOperationView::new(&linked, &validation, conditions)
        .map_err(|e| format!("Condiciones CAM invalidas: {e}"))?;
    Ok(Outcome {
        toolpath,
        linked,
        validation,
        conditions,
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
    tool_number: Option<u32>,
}

impl OperationEditorDraft {
    fn from_operation(op: &crate::project::operations::Operation) -> Self {
        Self {
            name: op.name.clone(),
            kind: op.kind.clone(),
            tool_id: op.tool_id.clone(),
            tool_number: op.tool_number,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 4096 {
            return Err("Escribe un nombre valido para la operacion".into());
        }
        if let OperationKind::ParallelTest(p) = &self.kind {
            p.validate()?;
        }
        if !crate::project::operations::nc_tool_number_valid(self.tool_number) {
            return Err("Numero CNC invalido (admitido T1..T9999)".into());
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
    result_provenances: HashMap<usize, CamProvenance>,
    message: String,
}

impl TestParallelPanel {
    /// Solo resultados con corte y validez estructural; la firma de herramienta
    /// y la procedencia CAM se invalidan por los sincronizadores existentes.
    pub fn nc_result(&self, index: usize) -> Option<(&PlannedToolpath, CuttingConditions)> {
        let outcome = self.results.get(&index)?;
        if !outcome.validation.is_structurally_valid() || outcome.validation.cutting_moves == 0 {
            return None;
        }
        Some((&outcome.linked, outcome.conditions))
    }
    /// Identidad exacta de la herramienta del calculo vigente.
    pub fn nc_tool_signature(&self, index: usize) -> Option<ToolSignature> {
        self.nc_result(index)?;
        Some(
            self.result_provenances
                .get(&index)?
                .tool_signature()
                .clone(),
        )
    }

    pub fn open_editor(&mut self, index: usize, project: &Project) {
        if let Some(op) = project.operations.items.get(index) {
            // Solo un editor: al editar otra operacion se descarta el borrador anterior.
            self.editing = Some(index);
            self.editor_draft = Some(OperationEditorDraft::from_operation(op));
            self.editor_tab = OperationEditorTab::General;
            self.editor_error.clear();
            self.confirm_delete_operation = false;
        }
    }

    /// El resultado no es parte del proyecto: al borrar una operacion hay
    /// que olvidar su vista previa y desplazar los indices de las posteriores.
    /// Cualquier calculo en curso se cancela para evitar asociar su resultado
    /// a otra operacion despues del cambio de indices.
    /// Cambia solo las claves de los resultados por indice, sin recalcular CAM.
    /// Un trabajo en curso mantiene su operacion despues de la reordenacion.
    pub fn operation_moved(&mut self, from: usize, to: usize) {
        use crate::project::operations::remap_index_after_move as remap;
        if from == to {
            return;
        }
        self.results = std::mem::take(&mut self.results)
            .into_iter()
            .map(|(index, result)| (remap(index, from, to), result))
            .collect();
        self.result_provenances = std::mem::take(&mut self.result_provenances)
            .into_iter()
            .map(|(index, stamp)| (remap(index, from, to), stamp))
            .collect();
        self.editing = self.editing.map(|index| remap(index, from, to));
        if let Some((index, _)) = self.active_job.as_mut() {
            *index = remap(*index, from, to);
        }
    }

    pub fn is_calculating(&self, index: usize) -> bool {
        self.active_job.as_ref().is_some_and(|(i, _)| *i == index)
    }

    pub fn calculation_running(&self) -> bool {
        self.active_job.is_some()
    }

    pub fn calculation_message(&self) -> &str {
        &self.message
    }

    /// Detiene el trabajo actual sin borrar resultados validos anteriores.
    pub fn cancel_current_calculation(&mut self) {
        if let Some((_, job)) = &self.active_job {
            job.cancel.store(true, Ordering::Relaxed);
            self.cancelling = true;
        }
    }

    /// Reutilizado por el editor y por las acciones directas del arbol.
    pub fn calculate_operation(
        &mut self,
        index: usize,
        project: &mut Project,
        stl_path: Option<&Path>,
        tool_library: &ToolLibrary,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if self.active_job.is_some() {
            return Err("Ya hay un calculo CAM en curso".into());
        }
        let operation = project
            .operations
            .items
            .get(index)
            .ok_or("La operacion ya no existe")?;
        let params = match &operation.kind {
            OperationKind::ParallelTest(params) => *params,
            OperationKind::Placeholder => return Err("Selecciona primero una estrategia".into()),
        };
        params.validate()?;
        let model = project
            .model
            .as_ref()
            .ok_or("No hay un modelo STL cargado")?;
        let path = stl_path.ok_or("No hay un STL de origen disponible")?;
        let tool = resolve_cutter(tool_library, operation.tool_id.as_deref(), 1.0)?;
        tool.ensure_supported()?;
        let provenance = CamProvenance::capture(project, index, path, tool.signature.clone())?;
        let affine = if project.model_transform.is_identity() {
            None
        } else {
            Some(project.model_transform.affine(&model.raw_bounds)?)
        };
        let mut effective = params;
        effective.diameter = tool.diameter_model();
        let plan =
            SamplingPlan::new(model.bounds, effective)?.with_stock(project.stock.as_ref())?;
        let job = Job::start(
            path.to_path_buf(),
            model.size_bytes,
            model.triangles,
            plan,
            tool,
            provenance,
            affine,
            ctx.clone(),
        )?;
        self.active_job = Some((index, job));
        self.cancelling = false;
        self.results.remove(&index);
        self.result_provenances.remove(&index);
        self.message = "Preparando motor y calculando...".into();
        if let Some(operation) = project.operations.items.get_mut(index) {
            operation.status = OperationStatus::NotCalculated;
        }
        ctx.request_repaint();
        Ok(())
    }

    pub fn operation_deleted(&mut self, deleted: usize) {
        self.active_job = None;
        self.cancelling = false;
        self.results = std::mem::take(&mut self.results)
            .into_iter()
            .filter_map(|(index, result)| index_after_delete(index, deleted).map(|i| (i, result)))
            .collect();
        self.result_provenances = std::mem::take(&mut self.result_provenances)
            .into_iter()
            .filter_map(|(index, provenance)| {
                index_after_delete(index, deleted).map(|i| (i, provenance))
            })
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
        self.result_provenances.clear();
        self.editing = None;
        self.editor_draft = None;
        self.editor_tab = OperationEditorTab::General;
        self.editor_error.clear();
        self.confirm_delete_operation = false;
        self.message.clear();
    }

    /// Contrasta cada resultado (y el trabajo en curso) con sus entradas reales.
    /// Los calculos sin procedencia nunca se conservan como vigentes.
    pub fn sync_tools(&mut self, project: &mut Project, library: &ToolLibrary) {
        let stale: Vec<usize> = self
            .results
            .keys()
            .copied()
            .filter(|&i| {
                self.result_provenances
                    .get(&i)
                    .is_none_or(|stamp| !stamp.is_current(project, i, library))
            })
            .collect();
        let cancel_running = self
            .active_job
            .as_ref()
            .is_some_and(|(i, job)| !job.provenance.is_current(project, *i, library));
        let had_stale = !stale.is_empty();
        for index in stale {
            self.results.remove(&index);
            self.result_provenances.remove(&index);
            if let Some(op) = project.operations.items.get_mut(index) {
                op.status = OperationStatus::NotCalculated;
            }
        }
        if cancel_running {
            let (index, _) = self.active_job.take().expect("Trabajo activo invalidado");
            self.cancelling = false;
            self.results.remove(&index);
            self.result_provenances.remove(&index);
            if let Some(op) = project.operations.items.get_mut(index) {
                op.status = OperationStatus::NotCalculated;
            }
        }
        if had_stale || cancel_running {
            self.message = "Cambio de STL, bruto, fresa o estrategia: CAM invalidado".into();
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
                    if !job.provenance.source_samples_current() {
                        self.results.remove(&index);
                        self.result_provenances.remove(&index);
                        if let Some(op) = project.operations.items.get_mut(index) {
                            op.status = OperationStatus::NotCalculated;
                        }
                        self.message =
                            "El STL cambio durante el calculo: resultado descartado".into();
                        return;
                    }
                    let status = validation_operation_status(&outcome.validation);
                    let first_issue = outcome
                        .validation
                        .issues
                        .first()
                        .map(|issue| format!(" Detalle: {}", issue.detail))
                        .unwrap_or_default();
                    self.message = format!(
                        "{} pasadas, {} puntos por pasada, {} contactos. {}. Aproximacion Z={:.3}, despeje Z={:.3}. Validacion: {}.{}",
                        outcome.lines_count,
                        outcome.samples_per_line,
                        outcome.contacts,
                        outcome.toolpath.pattern.label(),
                        outcome.linked.approach_z,
                        outcome.linked.clearance_z,
                        validation_label(&outcome.validation),
                        first_issue,
                    );
                    self.results.insert(index, outcome);
                    self.result_provenances
                        .insert(index, job.provenance.clone());
                    if let Some(op) = project.operations.items.get_mut(index) {
                        op.status = status;
                    }
                }
                Err(error) => {
                    self.results.remove(&index);
                    self.result_provenances.remove(&index);
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
        _stl_path: Option<&Path>,
        tool_library: &ToolLibrary,
        _available: bool,
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
        let units = crate::work_units::current(ctx);
        let mut accept = false;
        let mut cancel_edit = false;
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
                                                    format!(                                                "{} - {} Ø{}", tool.name,
                                                        tool.geometry.kind().label(),
                                                        units.length(tool.geometry.diameter_mm(), 3)),
                                                );
                                            }
                                        });
                                });
                                ui.separator();
                                ui.add_enabled_ui(!running, |ui| {
                                    ui.horizontal(|ui| {
                                        let mut assigned = draft.tool_number.is_some();
                                        if ui.checkbox(&mut assigned, "Asignar numero de herramienta CNC").changed() {
                                            draft.tool_number = assigned.then_some(1);
                                        }
                                        if let Some(number) = draft.tool_number.as_mut() {
                                            ui.add(
                                                egui::DragValue::new(number)
                                                    .range(1..=crate::project::operations::MAX_NC_TOOL_NUMBER)
                                                    .speed(1)
                                                    .prefix("T"),
                                            );
                                        } else {
                                            ui.small("Sin T asignado");
                                        }
                                    });
                                });                                if let Some(id) = draft.tool_id.as_deref() {
                                    if let Some(tool) = tool_library.get(id) {
                                                                                ui.small(format!("{} · Diametro Ø{} (biblioteca)",
                                            tool.geometry.kind().label(), units.length(tool.geometry.diameter_mm(), 3)));
                                        if !matches!(&tool.geometry, crate::tools::model::ToolGeometry::BallNose { .. } | crate::tools::model::ToolGeometry::Flat { .. } | crate::tools::model::ToolGeometry::BullNose { .. }) {
                                            ui.small("Este tipo aun no dispone de solucion de contacto CAM.");
                                        }
                                    } else {
                                        ui.colored_label(egui::Color32::YELLOW, "La fresa asignada no existe en la biblioteca.");
                                    }
                                }
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
                                                                                        ui.label(format!("Paso entre pasadas ({}):", units.symbol()));
                                            units.drag_mm(ui, &mut p.stepover, 0.5, Some((0.01, 1000.0)));
                                        });
                                        ui.horizontal(|ui| {
                                                                                        ui.label(format!("Paso longitudinal ({}):", units.symbol()));
                                            units.drag_mm(ui, &mut p.longitudinal, 0.1, Some((0.01, 1000.0)));
                                        });
                                        ui.horizontal(|ui| {
                                                                                        ui.label(format!("Sobreespesor ({}):", units.symbol()));
                                            units.drag_mm(ui, &mut p.allowance_mm, 0.1, Some((0.0, 1_000_000.0)));
                                        });
                                        ui.small("Separacion uniforme respecto al modelo nominal; no se acumula entre operaciones.");
                                        ui.separator();
                                        ui.small("Condiciones de corte de esta estrategia (pendientes por defecto):");
                                        ui.horizontal(|ui| {
                                            ui.label("Avance F de corte:");
                                            let mut defined = p.cutting_feed_mm_min.is_some();
                                            if ui.checkbox(&mut defined, "Definir").changed() {
                                                p.cutting_feed_mm_min = defined.then_some(0.0);
                                            }
                                            if let Some(feed) = p.cutting_feed_mm_min.as_mut() {
                                                                                                units.drag_mm(ui, feed, 10.0, Some((0.0, 1_000_000.0)));
                                                ui.label(format!("{}/min", units.symbol()));
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Husillo S:");
                                            let mut defined = p.spindle_rpm.is_some();
                                            if ui.checkbox(&mut defined, "Definir").changed() {
                                                p.spindle_rpm = defined.then_some(0.0);
                                            }
                                            if let Some(rpm) = p.spindle_rpm.as_mut() {
                                                ui.add(egui::DragValue::new(rpm)
                                                    .speed(100.0).range(0.0..=1_000_000.0)
                                                    .suffix(" rpm"));
                                            }
                                        });
                                        ui.small("F/S son datos de estrategia, no alteran el contacto STL ni autorizan G-code.");
                                        ui.horizontal(|ui| {
                                            ui.label("Direccion:");
                                            ui.radio_value(&mut p.along_x, true, "Eje X");
                                            ui.radio_value(&mut p.along_x, false, "Eje Y");
                                        });
                                        egui::ComboBox::from_label("Recorrido de pasadas")
                                            .selected_text(p.pass_pattern.label())
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut p.pass_pattern,
                                                    RasterPattern::Unidirectional,
                                                    RasterPattern::Unidirectional.label(),
                                                );
                                                ui.selectable_value(
                                                    &mut p.pass_pattern,
                                                    RasterPattern::Zigzag,
                                                    RasterPattern::Zigzag.label(),
                                                );
                                            });
                                        ui.separator();
                                        ui.horizontal(|ui| {
                                                                                        ui.label(format!("Despeje sobre bruto (+{}):", units.symbol()));
                                            units.drag_mm(ui, &mut p.clearance_mm, 0.5, Some((0.1, 1_000_000.0)));
                                        });
                                        ui.horizontal(|ui| {
                                                                                        ui.label(format!("Aproximacion sobre bruto (+{}):", units.symbol()));
                                            units.drag_mm(ui, &mut p.approach_mm, 0.1, Some((0.01, 1_000_000.0)));
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
                                                                                        ui.small(format!("Limites del modelo transformado, en {}.", units.symbol()));
                                            ui.horizontal(|ui| {
                                                ui.label("X min");
                                                units.drag_mm(ui, &mut area.x_min, 0.1, None);
                                                ui.label("X max");
                                                units.drag_mm(ui, &mut area.x_max, 0.1, None);
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Y min");
                                                units.drag_mm(ui, &mut area.y_min, 0.1, None);
                                                ui.label("Y max");
                                                units.drag_mm(ui, &mut area.y_max, 0.1, None);
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
                                .and_then(|plan| plan.with_stock(project.stock.as_ref()))
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
                if let Some(result) = self.results.get(&index) {
                                        let f = result.conditions.feed_mm_min
                        .map(|n| units.feed(n, 3))
                        .unwrap_or_else(|| "sin definir".into());
                    let s = result.conditions.spindle_rpm
                        .map(|n| format!("{n:.3} rpm"))
                        .unwrap_or_else(|| "sin definir".into());
                                        ui.small(format!("Condiciones de la ultima generacion: F={f}; S={s}; visualizacion: {}", units.symbol()));
                    if result.conditions.require_configured().is_err() {
                        ui.small("F/S pendientes: la previsualizacion funciona, pero el futuro postprocesado necesitara ambos valores.");
                    }
                }
                // No dependemos del mensaje global de la ultima operacion:
                // cada resultado lleva su propio informe, invalidado con el resultado.
                if let Some(result) = self.results.get(&index) {
                    let report = &result.validation;
                    ui.separator();
                    let color = if !report.is_structurally_valid() {
                        egui::Color32::LIGHT_RED
                    } else if report.cutting_moves == 0 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::LIGHT_GREEN
                    };
                    ui.colored_label(
                        color,
                        format!("G25.4b - Validacion: {}", validation_label(report)),
                    );
                    ui.small(format!(
                        "{} movimientos revisados, {} de corte, {} incidencias.",
                        report.checked_moves, report.cutting_moves, report.issues.len()
                    ));
                    for issue in report.issues.iter().take(8) {
                        let location = match (issue.pass_index, issue.move_index) {
                            (Some(pass), Some(movement)) => {
                                format!("Pasada {}, movimiento {}", pass + 1, movement + 1)
                            }
                            (_, Some(movement)) => format!("Movimiento {}", movement + 1),
                            _ => "Operacion".to_owned(),
                        };
                        let issue_color = match issue.severity {
                            Severity::Error => egui::Color32::LIGHT_RED,
                            Severity::Warning => egui::Color32::YELLOW,
                        };
                        ui.colored_label(
                            issue_color,
                            format!("{location} - {:?}: {}", issue.code, issue.detail),
                        );
                    }
                    if report.issues.len() > 8 {
                        ui.small(format!(
                            "{} incidencias adicionales (limite visual de 8).",
                            report.issues.len() - 8
                        ));
                    }
                    ui.small(
                        "Solo integridad estructural: NO verifica colisiones, material restante, fijaciones ni seguridad CNC. Sin G-code.",
                    );
                    // F1.2: las posiciones CNC se derivan del cero ACTUAL.
                    // No se cachean: mover el origen no obliga a recalcular el contacto.
                    // La trayectoria para OpenGL sigue en coordenadas STL/modelo.
                    if let (Some(model), Some(stock)) = (&project.model, &project.stock) {
                        match WorkFrame::new(&project.origin, stock, &model.bounds) {
                            Ok(frame) => {
                                if let (Some(first), Some(last)) =
                                    (result.linked.moves.first(), result.linked.moves.last())
                                {
                                    match (
                                        frame.model_to_work(first.from),
                                        frame.model_to_work(last.to),
                                    ) {
                                        (Ok(start), Ok(end)) => {
                                            ui.small(format!(
                                                                                                "F1.2 - Origen trabajo ({}): inicio X{:.3} Y{:.3} Z{:.3}",
                                                units.symbol(), units.display(start[0]), units.display(start[1]), units.display(start[2])
                                            ));
                                            ui.small(format!(
                                                                                                "F1.2 - Fin X{:.3} Y{:.3} Z{:.3} (solo referencia)",
                                                units.display(end[0]), units.display(end[1]), units.display(end[2])
                                            ));
                                        }
                                        _ => {
                                            ui.colored_label(
                                                egui::Color32::LIGHT_RED,
                                                "F1.2 - Error al convertir los extremos al cero de trabajo",
                                            );
                                        }
                                    }
                                } else {
                                    ui.small("F1.2 - Sin movimientos que transformar al origen de trabajo");
                                }
                            }
                            Err(error) => {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!("F1.2 - Origen de trabajo: {error}"),
                                );
                            }
                        }
                    }
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
        if accept {
            // Aplicar una unica vez: el proyecto solo cambia al aceptar o generar.
            if let Some(op) = project.operations.items.get_mut(index) {
                let modified =
                    op.name != draft.name || op.kind != draft.kind || op.tool_id != draft.tool_id;
                op.tool_number = draft.tool_number;
                if modified {
                    op.name = draft.name.clone();
                    op.kind = draft.kind.clone();
                    op.tool_id = draft.tool_id.clone();
                    op.status = OperationStatus::NotCalculated;
                    self.results.remove(&index);
                    self.result_provenances.remove(&index);
                    self.message = "Configuracion cambiada. Pendiente de recalcular.".into();
                }
            }
            self.editor_error.clear();
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

    /// Una pareja de SOLIDOS 3D por operacion visible, usando los extremos reales
    /// de la secuencia. El visor normaliza las coordenadas solo para el render.
    pub fn marker_instances_3d(
        &self,
        model: &ModelReference,
        project: &Project,
    ) -> Vec<crate::cam_markers_3d::Marker3d> {
        use crate::cam_markers_3d::{Marker3d, MarkerKind};
        let mut markers = Vec::with_capacity(self.results.len().saturating_mul(2));
        let mut indices: Vec<usize> = self.results.keys().copied().collect();
        indices.sort_unstable();
        for index in indices {
            if !project
                .operations
                .items
                .get(index)
                .is_some_and(|op| op.visible)
            {
                continue;
            }
            let path = &self.results[&index].linked;
            let Some((first, last)) = operation_marker_positions(path) else {
                continue;
            };
            if let (Some(start), Some(finish)) =
                (model.visual_point(first), model.visual_point(last))
            {
                markers.push(Marker3d {
                    position: start,
                    kind: MarkerKind::StartCone,
                    direction: first_marker_motion_direction(path, model),
                });
                markers.push(Marker3d {
                    position: finish,
                    kind: MarkerKind::FinishSphere,
                    direction: Vec3::Z,
                });
            }
        }
        markers
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Contrato de render existente: camara y contexto explicitos; no modificar la geometria"
    )]
    pub fn paint(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        model: &ModelReference,
        project: &Project,
        yaw: f32,
        pitch: f32,
        zoom: f32,
        pan: Vec3,
        projection: crate::navigation::Projection,
    ) {
        if self.results.is_empty() {
            return;
        }
        let mvp = crate::navigation::view_projection_mode(
            yaw,
            pitch,
            zoom,
            rect.width() / rect.height().max(1.0),
            pan,
            projection,
        );
        let painter = ui.painter().with_clip_rect(rect);
        let project_point = |xyz: [f64; 3]| -> Option<egui::Pos2> {
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
            if !project
                .operations
                .items
                .get(index)
                .is_some_and(|op| op.visible)
            {
                continue;
            }
            let result = &self.results[&index];
            let blue = egui::Color32::from_rgb(45, 150, 250);
            let red = egui::Color32::from_rgb(244, 66, 76);
            // Pintar primero enlaces propuestos azules y despues movimientos
            // controlados rojos. No implican que un G0 sea seguro en maquina.
            for rapid_pass in [true, false] {
                for movement in &result.linked.moves {
                    let rapid = movement.kind == PlannedMoveKind::RapidTraverse;
                    if rapid != rapid_pass {
                        continue;
                    }
                    if let (Some(a), Some(b)) =
                        (project_point(movement.from), project_point(movement.to))
                    {
                        let color = if rapid { blue } else { red };
                        painter.line_segment([a, b], egui::Stroke::new(1.8, color));
                    }
                }
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(12.0, 64.0),
            egui::Align2::LEFT_TOP,
            format!(
                "PARALELO TEST · {} operaciones · G0 propuesto AZUL · aproximacion/corte/retirada ROJO · NO ejecutar",
                self.results.len()
            ),
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
    }
}

/// Los extremos pertenecen a la secuencia TOTAL de movimientos de la operacion,
/// no al inicio/final de cada pasada ni al primer/ultimo punto de corte.
/// Direccion 3D del primer movimiento efectivo, no la orientacion de camara.
/// La malla del cono tiene base en el inicio y punta en el sentido de avance.
fn first_marker_motion_direction(path: &PlannedToolpath, model: &ModelReference) -> Vec3 {
    for movement in &path.moves {
        let (Some(from), Some(to)) = (
            model.visual_point(movement.from),
            model.visual_point(movement.to),
        ) else {
            continue;
        };
        let delta = to - from;
        if delta.is_finite() && delta.length_squared() > 1.0e-12 {
            return delta.normalize();
        }
    }
    // Si no hay ningun desplazamiento representable, usar la vertical de entrada.
    Vec3::NEG_Z
}

fn operation_marker_positions(path: &PlannedToolpath) -> Option<([f64; 3], [f64; 3])> {
    Some((path.moves.first()?.from, path.moves.last()?.to))
}

#[cfg(test)]
mod g25_3_cone_direction_tests {
    use super::*;
    use cam_libre_visor::toolpath_links::PlannedMove;

    fn reference() -> ModelReference {
        ModelReference {
            name: "marker_test".into(),
            bounds: Bounds {
                min: [0.0, 0.0, 0.0],
                max: [10.0, 10.0, 10.0],
            },
            raw_bounds: Bounds {
                min: [0.0, 0.0, 0.0],
                max: [10.0, 10.0, 10.0],
            },
            triangles: 1,
            size_bytes: 0,
        }
    }

    #[test]
    fn cone_points_along_first_nonzero_motion_not_upwards() {
        let path = PlannedToolpath {
            moves: vec![
                PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from: [2.0, 3.0, 8.0],
                    to: [2.0, 3.0, 8.0],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from: [2.0, 3.0, 8.0],
                    to: [2.0, 3.0, 6.0],
                    pass_index: 0,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            first_marker_motion_direction(&path, &reference()),
            Vec3::NEG_Z
        );
        let horizontal = PlannedToolpath {
            moves: vec![PlannedMove {
                kind: PlannedMoveKind::Cutting,
                from: [2.0, 3.0, 6.0],
                to: [4.0, 3.0, 6.0],
                pass_index: 0,
            }],
            ..Default::default()
        };
        assert_eq!(
            first_marker_motion_direction(&horizontal, &reference()),
            Vec3::X
        );
        assert_eq!(
            first_marker_motion_direction(&PlannedToolpath::default(), &reference()),
            Vec3::NEG_Z
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
    fn sampling_above_legacy_test_cap_is_allowed() {
        let plan = SamplingPlan::new(
            bounds(),
            Parameters {
                stepover: 0.05,
                longitudinal: 0.05,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((plan.lines, plan.samples), (121, 201));
        assert_eq!(plan.total(), 24_321);
    }

    #[test]
    fn excessive_sampling_is_still_rejected_before_allocation() {
        let error = SamplingPlan::new(
            bounds(),
            Parameters {
                stepover: 0.01,
                longitudinal: 0.001,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("limite tecnico"));
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
                pass_pattern: RasterPattern::Unidirectional,
                clearance_mm: 10.0,
                approach_mm: 2.0,
                cutting_feed_mm_min: None,
                spindle_rpm: None,
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
                pass_pattern: RasterPattern::Unidirectional,
                clearance_mm: 10.0,
                approach_mm: 2.0,
                cutting_feed_mm_min: None,
                spindle_rpm: None,
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
        let mut params = Parameters {
            longitudinal: 0.00001,
            ..Default::default()
        };
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
                None,
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
        let p = Parameters {
            area_xy: Some(TestAreaXY {
                x_min: 2.0,
                x_max: 7.0,
                y_min: 12.0,
                y_max: 15.0,
            }),
            stepover: 4.0,
            longitudinal: 3.0,
            ..Default::default()
        };
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
        let p = Parameters {
            area_xy: Some(TestAreaXY {
                x_min: -1.0,
                x_max: 7.0,
                y_min: 12.0,
                y_max: 15.0,
            }),
            ..Default::default()
        };
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
    fn calculate_without_strategy_refuses_to_start() {
        let mut project = Project::default();
        let index = project.operations.add_new().unwrap();
        let mut panel = TestParallelPanel::default();
        let error = panel
            .calculate_operation(
                index,
                &mut project,
                None,
                &ToolLibrary::default(),
                &egui::Context::default(),
            )
            .unwrap_err();
        assert!(error.contains("estrategia"));
        assert!(!panel.calculation_running());
    }

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
            tool_number: None,
        };
        assert!(draft.validate().is_err());
        draft.name = "Operacion A".into();
        assert!(draft.validate().is_ok());
        draft.tool_number = Some(0);
        assert!(draft.validate().is_err());
        draft.tool_number = Some(12);
        assert!(draft.validate().is_ok());
        let p = Parameters {
            stepover: -1.0,
            ..Default::default()
        };
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
    fn moving_operations_preserves_editor_index() {
        let mut panel = TestParallelPanel {
            editing: Some(2),
            ..Default::default()
        };
        panel.operation_moved(2, 0);
        assert_eq!(panel.editing, Some(0));
        panel.operation_moved(0, 2);
        assert_eq!(panel.editing, Some(2));
    }

    #[test]
    fn deletion_removes_the_entry_and_reindexes_later_operations() {
        assert_eq!(index_after_delete(0, 1), Some(0));
        assert_eq!(index_after_delete(1, 1), None);
        assert_eq!(index_after_delete(2, 1), Some(1));
        assert_eq!(index_after_delete(3, 1), Some(2));
        let mut panel = TestParallelPanel {
            editing: Some(3),
            ..Default::default()
        };
        panel.operation_deleted(1);
        assert_eq!(panel.editing, Some(2));
        assert!(panel.result_provenances.is_empty());
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
        let bounds = Bounds {
            min: [0.0; 3],
            max: [3.0; 3],
        };
        project.model = Some(crate::project::model::ModelReference {
            name: "patron.stl".into(),
            bounds,
            raw_bounds: bounds,
            triangles: 1,
            size_bytes: 134,
        });
        project.stock = Some(crate::project::stock::Stock::from_bounds(&bounds));
        let index = project.operations.add_new().unwrap();
        project.operations.items[index].kind = OperationKind::ParallelTest(Parameters::default());
        project.operations.items[index].tool_id = Some(id.clone());
        project.operations.items[index].status = OperationStatus::Ready;
        let stl_path =
            std::env::temp_dir().join(format!("lagucam_p1a_sync_{}.stl", std::process::id()));
        let mut bytes = vec![0u8; 134];
        bytes[80..84].copy_from_slice(&1u32.to_le_bytes());
        fs::write(&stl_path, bytes).unwrap();
        let mut panel = TestParallelPanel::default();
        panel.results.insert(
            index,
            Outcome {
                toolpath: Toolpath::default(),
                linked: PlannedToolpath::default(),
                validation: ValidationReport::default(),
                conditions: CuttingConditions::default(),
                contacts: 0,
                lines_count: 0,
                samples_per_line: 0,
            },
        );
        let provenance = CamProvenance::capture(&project, index, &stl_path, signature).unwrap();
        panel.result_provenances.insert(index, provenance);
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
        fs::remove_file(stl_path).unwrap();
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
        let params = Parameters {
            allowance_mm: 2.5,
            ..Default::default()
        };
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
            let mut params = Parameters {
                diameter: 4.0,
                stepover: 5.0,
                longitudinal: 5.0,
                allowance_mm: 0.0,
                ..Default::default()
            };
            let nominal_plan = SamplingPlan::new(bounds, params).unwrap();
            let original = compute(
                &file,
                bytes.len() as u64,
                1,
                &nominal_plan,
                cutter,
                None,
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
                None,
                &cancel,
                &AtomicUsize::new(0),
            )
            .unwrap();
            // El mismo contacto debe atravesar la conversión a G25.1 sin cambiar Z.
            let nominal_z = original.toolpath.segments[0].points[0][2];
            let roughing_z = roughing.toolpath.segments[0].points[0][2];
            assert_eq!(roughing.toolpath.raster_passes, roughing.lines_count);
            assert_eq!(
                roughing.toolpath.samples_per_pass,
                roughing.samples_per_line
            );
            assert!((nominal_z - 5.0).abs() < 1.0e-10, "{cutter:?}: {nominal_z}");
            assert!(
                (roughing_z - 7.0).abs() < 1.0e-10,
                "{cutter:?}: {roughing_z}"
            );
        }
    }
}

#[cfg(test)]
mod g25_2_pipeline_tests {
    use super::*;

    #[test]
    fn sampling_preserves_same_xy_and_contact_query_for_both_patterns() {
        let area = Bounds {
            min: [5.0, 5.0, 0.0],
            max: [15.0, 15.0, 10.0],
        };
        let mut params = Parameters {
            diameter: 4.0,
            longitudinal: 10.0,
            stepover: 10.0,
            ..Default::default()
        };
        let nominal = SamplingPlan::new(area, params).unwrap();
        params.pass_pattern = RasterPattern::Zigzag;
        let zigzag = SamplingPlan::new(area, params).unwrap();
        let cutter = CutterGeometry::Flat {
            diameter_model: 4.0,
        };
        for line in 0..2 {
            for sample in 0..2 {
                assert_eq!(nominal.xy(line, sample), zigzag.xy(line, sample));
                let a = nominal.contact_query(line, sample, cutter);
                let b = zigzag.contact_query(line, sample, cutter);
                assert_eq!(a.x_model.to_bits(), b.x_model.to_bits());
                assert_eq!(a.y_model.to_bits(), b.y_model.to_bits());
                assert_eq!(a.allowance_model.to_bits(), b.allowance_model.to_bits());
                assert_eq!(a.cutter, b.cutter);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn real_solver_preserves_contact_heights_but_reverses_second_pass() {
        // Triangulo horizontal grande: las cuatro consultas tienen contacto.
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 12]);
        for p in [[0.0f32, 0.0, 5.0], [100.0, 0.0, 5.0], [0.0, 100.0, 5.0]] {
            for v in p {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
        let path = std::env::temp_dir().join(format!("lagucam-g25-2-{}.stl", uuid::Uuid::new_v4()));
        std::fs::write(&path, &bytes).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());

        let bounds = Bounds {
            min: [5.0, 5.0, 5.0],
            max: [15.0, 15.0, 5.0],
        };
        let mut params = Parameters {
            diameter: 4.0,
            longitudinal: 10.0,
            stepover: 10.0,
            ..Default::default()
        };
        let cutter = CutterGeometry::Flat {
            diameter_model: 4.0,
        };
        let cancel = AtomicBool::new(false);
        let normal = compute(
            &path,
            bytes.len() as u64,
            1,
            &SamplingPlan::new(bounds, params).unwrap(),
            cutter,
            None,
            &cancel,
            &AtomicUsize::new(0),
        )
        .unwrap();
        params.pass_pattern = RasterPattern::Zigzag;
        let alternating = compute(
            &path,
            bytes.len() as u64,
            1,
            &SamplingPlan::new(bounds, params).unwrap(),
            cutter,
            None,
            &cancel,
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert_eq!(normal.contacts, alternating.contacts);
        assert_eq!(normal.contacts, 4);
        assert_eq!(normal.toolpath.segments.len(), 2);
        assert_eq!(alternating.toolpath.segments.len(), 2);
        assert_eq!(
            normal.toolpath.segments[0].points,
            alternating.toolpath.segments[0].points
        );
        let forward = &normal.toolpath.segments[1].points;
        let backwards = &alternating.toolpath.segments[1].points;
        assert_eq!(
            backwards,
            &forward.iter().copied().rev().collect::<Vec<_>>()
        );
        assert_eq!(alternating.toolpath.segments[1].first_sample, 1);
        assert!(
            backwards
                .iter()
                .all(|p| p[2].to_bits() == 5.0_f64.to_bits())
        );
    }
}

#[cfg(test)]
mod g25_3_stock_and_plan_tests {
    use super::*;

    #[test]
    fn stock_top_is_not_ignored_and_invalid_stock_is_an_error() {
        let bounds = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 10.0, 4.0],
        };
        let plan = SamplingPlan::new(bounds, Parameters::default()).unwrap();
        assert_eq!(plan.reference_z, 4.0);
        let mut stock = crate::project::stock::Stock::from_bounds(&bounds);
        stock.size[2] = 12.0;
        let raised = plan.clone().with_stock(Some(&stock)).unwrap();
        assert_eq!(raised.reference_z, 12.0);
        stock.min[2] = f64::MAX;
        assert!(plan.with_stock(Some(&stock)).is_err());
    }
}

#[cfg(test)]
mod g25_3_global_marker_tests {
    use super::*;

    #[test]
    fn markers_follow_first_and_last_move_even_with_two_zigzag_passes() {
        let rows = vec![
            vec![Some([0.0, 0.0, 1.0]), Some([1.0, 0.0, 1.0])],
            vec![Some([0.0, 1.0, 2.0]), Some([1.0, 1.0, 2.0])],
        ];
        let path =
            Toolpath::from_contact_grid_with_pattern(rows, 4, RasterPattern::Zigzag).unwrap();
        let movements = PlannedToolpath::from_toolpath(&path, 3.0, 2.0, 10.0).unwrap();
        assert_eq!(
            operation_marker_positions(&movements),
            Some(([0.0, 0.0, 13.0], [0.0, 1.0, 13.0]))
        );
        assert!(movements.moves.len() > 2);
    }

    #[test]
    fn empty_operation_has_no_start_or_finish_marker() {
        assert!(operation_marker_positions(&PlannedToolpath::default()).is_none());
    }
}

#[cfg(test)]
mod g25_4b_integration_tests {
    use super::*;

    fn make_raster(with_cuts: bool) -> Toolpath {
        let points = if with_cuts {
            vec![vec![Some([0.0, 0.0, 1.0]), Some([1.0, 0.0, 1.5])]]
        } else {
            vec![vec![None, None]]
        };
        Toolpath::from_contact_grid_with_pattern(points, 2, RasterPattern::Unidirectional).unwrap()
    }

    fn test_outcome(with_cuts: bool) -> Outcome {
        let toolpath = make_raster(with_cuts);
        let linked = PlannedToolpath::from_toolpath(&toolpath, 4.0, 2.0, 10.0).unwrap();
        let validation = validate_planned_toolpath(&linked);
        Outcome {
            toolpath,
            linked,
            validation,
            conditions: CuttingConditions::default(),
            contacts: if with_cuts { 2 } else { 0 },
            lines_count: 1,
            samples_per_line: 2,
        }
    }

    #[test]
    fn generated_cutting_path_has_a_report_and_ready_status() {
        let result = test_outcome(true);
        assert!(result.validation.is_structurally_valid());
        assert_eq!(result.validation.checked_moves, result.linked.moves.len());
        assert!(result.validation.cutting_moves > 0);
        assert_eq!(
            validation_operation_status(&result.validation),
            OperationStatus::Ready
        );
        assert_eq!(
            validation_label(&result.validation),
            "CORRECTA estructuralmente"
        );
    }

    #[test]
    fn invalid_toolpath_is_reported_without_changing_geometry() {
        let mut result = test_outcome(true);
        let untouched = result.linked.clone();
        result.linked.moves[0].to[0] += 1.0; // aproximacion oblicua, invalida
        result.validation = validate_planned_toolpath(&result.linked);
        assert_eq!(
            validation_operation_status(&result.validation),
            OperationStatus::Error
        );
        assert!(
            result
                .validation
                .issues
                .iter()
                .any(|x| x.move_index == Some(0))
        );
        assert_eq!(result.toolpath.contact_points, 2);
        assert_ne!(result.linked, untouched); // la alteracion proviene solo del test
    }

    #[test]
    fn no_cut_is_a_warning_and_never_marked_ready() {
        let result = test_outcome(false);
        assert!(result.validation.is_structurally_valid());
        assert_eq!(result.validation.cutting_moves, 0);
        assert_eq!(
            validation_operation_status(&result.validation),
            OperationStatus::NotCalculated
        );
        assert_eq!(
            validation_label(&result.validation),
            "SIN MECANIZADO (aviso)"
        );
    }

    #[test]
    fn report_is_kept_per_operation_and_reindexed_on_deletion() {
        let mut panel = TestParallelPanel::default();
        panel.results.insert(0, test_outcome(false));
        panel.results.insert(1, test_outcome(true));
        assert_eq!(panel.results[&0].validation.cutting_moves, 0);
        assert_eq!(panel.results[&1].validation.cutting_moves, 1);
        panel.operation_deleted(0);
        assert_eq!(panel.results.len(), 1);
        assert_eq!(panel.results[&0].validation.cutting_moves, 1);
    }
}

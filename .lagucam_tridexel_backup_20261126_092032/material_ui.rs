//! P2-D2: previsualizacion de material 2.5D a partir del G-code interpretado.
//! Resolucion del calculo independiente del tamano STL; dibujo desacoplado y
//! simplificado para no crear cientos de miles de triangulos egui por frame.

use crate::sparse_runtime::SparseRuntime;
use crate::sparse_scene_gpu::Surface;
use crate::{
    project::{Project, cam_coordinates::WorkFrame},
    tools::model::{Tool, ToolGeometry, ToolLibrary},
};
use cam_libre_visor::sparse_material::MaterialRegion;
use cam_libre_visor::{
    material_sim::{
        CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox, segments_from_trace,
    },
    nc_contract::NcTrace,
    sparse_material::{SparseConfig, SparseMaterial},
};
use eframe::egui;
use std::cell::RefCell;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    time::Duration,
};

enum MaterialResult {
    Dense(MaterialHeightfield),
    Sparse(SparseMaterial, Vec<CutSegment>),
}

struct MaterialJob {
    rx: Receiver<Result<MaterialResult, String>>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    total: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    tool: Tool,
    stock: StockBox,
}

pub struct MaterialUi {
    selected_tool_id: Option<String>,
    resolution: usize,
    use_sparse: bool,
    spacing_mm: f64,
    visible: bool,
    isolated_view: bool,
    history_budget_mib: usize,
    field: Option<MaterialHeightfield>,
    sparse_field: Option<SparseMaterial>,
    sparse_segments: Vec<CutSegment>,
    sparse_cache: RefCell<Option<(f64, Arc<Surface>)>>,
    sparse_runtime: Option<SparseRuntime>,
    sparse_display_cursor: f64,
    sparse_pending: RefCell<Vec<MaterialRegion>>,
    job: Option<MaterialJob>,
    snapshot: Option<Snapshot>,
    message: String,
}

impl Default for MaterialUi {
    fn default() -> Self {
        Self {
            selected_tool_id: None,
            resolution: 256,
            use_sparse: true,
            spacing_mm: 0.05,
            visible: true,
            isolated_view: true,
            history_budget_mib: 256,
            field: None,
            sparse_field: None,
            sparse_segments: Vec::new(),
            sparse_cache: RefCell::new(None),
            sparse_runtime: None,
            sparse_display_cursor: 0.0,
            sparse_pending: RefCell::new(Vec::new()),
            job: None,
            snapshot: None,
            message: String::new(),
        }
    }
}

fn supported_spec(tool: &Tool) -> Option<CutterSpec> {
    let shape = match &tool.geometry {
        ToolGeometry::Flat { .. } => CutterShape::Flat,
        ToolGeometry::BallNose { .. } => CutterShape::Ball,
        _ => return None,
    };
    tool.validate().ok()?;
    Some(CutterSpec {
        shape,
        diameter_mm: tool.geometry.diameter_mm(),
        cutting_length_mm: tool.cutting_length_mm,
    })
}

impl MaterialUi {
    /// Preparar el material desde el mismo NC interpretado que se reproduce.
    /// Si la geometria de la fresa no es compatible, el reproductor continua
    /// mostrando la trayectoria sin inventar una fresa equivalente.
    pub fn prepare_compact(
        &mut self,
        project: &Project,
        library: &ToolLibrary,
        trace: &NcTrace,
    ) -> Result<(), String> {
        self.start(project, library, trace)
    }

    /// El visor compacto mantiene al dia el material incluso sin panel inferior.
    pub fn tick_compact(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        library: &ToolLibrary,
        playhead: f64,
    ) {
        self.refresh(project, library);
        self.poll(ctx);
        if let Some(field) = &mut self.field {
            field.seek(playhead);
        }
        self.tick_sparse_worker(ctx, playhead);
    }
    fn tick_sparse_worker(&mut self, ctx: &egui::Context, playhead: f64) {
        let Some(runtime) = self.sparse_runtime.as_ref() else {
            return;
        };
        runtime.request(playhead);
        // Procesar lotes entregados, sin esperar al hilo de simulacion.
        for _ in 0..12 {
            match runtime.receive() {
                Ok(Some(frame)) => {
                    let changed_blocks = frame.regions.len();
                    self.sparse_display_cursor = frame.cursor;
                    if let Some((_, surface)) = self.sparse_cache.borrow().as_ref() {
                        surface.queue_regions(frame.regions);
                    } else {
                        self.sparse_pending.borrow_mut().extend(frame.regions);
                    }
                    self.message = format!(
                        "Material NC {:.2} / {:.2} | {} bloques modificados",
                        frame.cursor, playhead, changed_blocks
                    );
                    ctx.request_repaint();
                }
                Ok(None) => break,
                Err(error) => {
                    self.message = format!("ERROR trabajador de material: {error}");
                    break;
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(24));
    }
    /// Herramienta y altura deben mostrar el mismo estado NC confirmado.
    /// El reloj puede solicitar estados futuros, pero no adelantamos la fresa.
    pub fn visual_cursor(&self, requested: f64) -> f64 {
        if self.visible && self.sparse_runtime.is_some() {
            self.sparse_display_cursor
        } else {
            requested
        }
    }
    /// Estado visible incluso con reproductor NC compacto.
    pub fn compact_status(&self) -> String {
        if let Some(job) = &self.job {
            return format!(
                "Material: calculando {} / {} movimientos NC",
                job.progress.load(Ordering::Relaxed),
                job.total
            );
        }
        if self.sparse_field.is_some() {
            return format!("Material disperso: {}", self.message);
        }
        if self.field.is_some() {
            return format!("Material clasico: {}", self.message);
        }
        if !self.message.is_empty() {
            return self.message.clone();
        }
        "Material: no calculado; bruto provisional".into()
    }
    /// Al cambiar de programa, no reutilizar una seleccion de su programa anterior.
    /// Para NC generado se propone la fresa de su operacion; se puede cambiar a mano.
    pub fn select_source_tool(&mut self, tool_id: Option<&str>) {
        self.clear();
        self.selected_tool_id = tool_id.map(str::to_owned);
    }

    pub fn selected_tool_id(&self) -> Option<&str> {
        self.selected_tool_id.as_deref()
    }

    /// Vista aislada solo si existe material calculado y visible.
    /// La herramienta tiene la misma identidad y geometria que la simulacion calculada.
    /// Si el resultado se invalida, no se muestra una fresa obsoleta.
    pub fn simulated_tool(&self) -> Option<&Tool> {
        if !self.visible {
            return None;
        }
        self.snapshot.as_ref().map(|snapshot| &snapshot.tool)
    }
    /// El material calculado ya sincronizado con el cursor NC; nunca devuelve un job parcial.
    pub fn render_field(&self) -> Option<&MaterialHeightfield> {
        if !self.visible || self.job.is_some() {
            return None;
        }
        self.field.as_ref()
    }
    pub fn isolated_view_active(&self) -> bool {
        self.isolated_view
            && self.visible
            && (self.field.is_some() || self.sparse_field.is_some())
            && self.job.is_none()
    }

    /// La superficie CPU solo se reconstruye si cambia el cursor NC.
    /// El renderer conserva los buffers GPU al mover la camara.
    /// Rejilla implicita y textura residentes: se construye SOLO al iniciar sesion.
    pub fn render_sparse_surface(&self, project: &Project) -> Result<Option<Arc<Surface>>, String> {
        if !self.visible || self.job.is_some() {
            return Ok(None);
        }
        let Some(field) = self.sparse_field.as_ref() else {
            return Ok(None);
        };
        if let Some((_, existing)) = self.sparse_cache.borrow().as_ref() {
            return Ok(Some(Arc::clone(existing)));
        }
        let built = Arc::new(Surface::build(project, field)?);
        built.queue_regions(std::mem::take(&mut *self.sparse_pending.borrow_mut()));
        *self.sparse_cache.borrow_mut() = Some((0.0, Arc::clone(&built)));
        Ok(Some(built))
    }

    pub fn clear(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.field = None;
        self.sparse_field = None;
        self.sparse_segments.clear();
        self.sparse_runtime = None;
        self.sparse_display_cursor = 0.0;
        self.sparse_pending.borrow_mut().clear();
        *self.sparse_cache.borrow_mut() = None;
        self.snapshot = None;
        self.message.clear();
    }

    fn current_snapshot(&self, project: &Project, library: &ToolLibrary) -> Option<Snapshot> {
        let tool = library.get(self.selected_tool_id.as_ref()?).cloned()?;
        let stock = project.stock.as_ref()?;
        Some(Snapshot {
            tool,
            stock: StockBox {
                min_mm: stock.min,
                size_mm: stock.size,
            },
        })
    }

    fn start(
        &mut self,
        project: &Project,
        library: &ToolLibrary,
        trace: &NcTrace,
    ) -> Result<(), String> {
        self.clear();
        let snapshot = self
            .current_snapshot(project, library)
            .ok_or("Selecciona una fresa y define un bruto")?;
        let cutter = supported_spec(&snapshot.tool)
            .ok_or("El prototipo 2.5D admite unicamente fresas planas o esfericas")?;
        snapshot.stock.validate()?;
        let model = project.model.as_ref().ok_or("Abre un modelo STL")?;
        let stock = project.stock.as_ref().ok_or("Define un bruto")?;
        // Debe coincidir con el limite de 3 ejes del postprocesador.
        WorkFrame::new(&project.origin, stock, &model.bounds)
            .map_err(|error| format!("Origen CNC incompatible con 3 ejes: {error}"))?;
        let segments = segments_from_trace(trace, |work_xyz| {
            project
                .origin
                .work_to_model(stock, &model.bounds, work_xyz)
                .map_err(|error| format!("Transformacion NC -> modelo: {error}"))
        })?;
        let total = segments.len();
        let resolution = self.resolution;
        let budget_mib = self.history_budget_mib;
        let use_sparse = self.use_sparse;
        let spacing_mm = self.spacing_mm;
        let stock_spec = snapshot.stock;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_cancel = Arc::clone(&cancel);
        let worker_progress = Arc::clone(&progress);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("lagucam-heightfield".into())
            .spawn(move || {
                let result = (|| -> Result<MaterialResult, String> {
                    if use_sparse {
                        let mut field = SparseMaterial::new(SparseConfig {
                            stock: stock_spec,
                            cutter,
                            spacing_mm,
                            checkpoint_every: 25,
                            max_checkpoints: 4,
                            height_budget_mib: budget_mib,
                        })?;
                        if worker_cancel.load(Ordering::Relaxed) {
                            return Err("Calculo cancelado".into());
                        }
                        // El estado inicial debe ser un bruto intacto: no calcular
                        // todo el NC antes de mostrar la herramienta y reproducir.
                        // Los movimientos se aplican mediante seek en reproduccion.
                        worker_progress.store(0, Ordering::Relaxed);
                        Ok(MaterialResult::Sparse(field, segments))
                    } else {
                        MaterialHeightfield::simulate_with_budget(
                            stock_spec,
                            cutter,
                            resolution,
                            budget_mib,
                            &segments,
                            &worker_cancel,
                            &worker_progress,
                        )
                        .map(MaterialResult::Dense)
                    }
                })();
                let _ = tx.send(result);
            })
            .map_err(|error| format!("No se pudo iniciar simulacion: {error}"))?;
        self.job = Some(MaterialJob {
            rx,
            cancel,
            progress,
            total,
        });
        self.snapshot = Some(snapshot);
        self.message = "Preparando barrido de material desde el NC interpretado...".into();
        Ok(())
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let received = self.job.as_ref().map(|job| job.rx.try_recv());
        match received {
            Some(Ok(Ok(MaterialResult::Dense(field)))) => {
                self.field = Some(field);
                self.job = None;
                self.message = "Simulacion 2.5D preparada (motor anterior).".into();
                ctx.request_repaint();
            }
            Some(Ok(Ok(MaterialResult::Sparse(field, segments)))) => {
                let fresh = field.fresh();
                self.job = None;
                match fresh {
                    Ok(initial) => match SparseRuntime::new(field, segments) {
                        Ok(runtime) => {
                            self.sparse_field = Some(initial);
                            self.sparse_runtime = Some(runtime);
                            self.sparse_segments.clear();
                            self.sparse_pending.borrow_mut().clear();
                            *self.sparse_cache.borrow_mut() = None;
                            self.message = "Material asincrono preparado; bruto intacto".into();
                        }
                        Err(error) => self.message = format!("ERROR iniciando material: {error}"),
                    },
                    Err(error) => self.message = format!("ERROR creando bruto: {error}"),
                }
                ctx.request_repaint();
            }
            Some(Ok(Err(error))) => {
                self.field = None;
                self.job = None;
                self.message = format!("ERROR en simulacion 2.5D: {error}");
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.field = None;
                self.job = None;
                self.message = "ERROR: termino el calculo de material sin resultado".into();
            }
            Some(Err(TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(90));
            }
            None => {}
        }
    }

    /// Comprueba dependencias incluso cuando no se muestra la pestana Simulacion.
    pub fn refresh(&mut self, project: &Project, library: &ToolLibrary) {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|before| self.current_snapshot(project, library).as_ref() != Some(before))
        {
            self.clear();
            self.message = "Fresa o bruto modificado: recalcula el material".into();
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        library: &ToolLibrary,
        trace: &NcTrace,
        playhead: f64,
    ) {
        self.refresh(project, library);
        self.poll(ui.ctx());
        if let Some(field) = &mut self.field {
            field.seek(playhead);
        }
        self.tick_sparse_worker(ui.ctx(), playhead);
        ui.collapsing("Simulacion de arranque de material · Heightfield 2.5D", |ui| {
            ui.colored_label(egui::Color32::YELLOW,
                "Vista aproximada para fresado 3 ejes vertical. No simula socavados, mango, fijaciones ni seguridad CNC.");
            let previous_tool = self.selected_tool_id.clone();
            let selected = self.selected_tool_id.as_ref().and_then(|id| library.get(id))
                .map_or("Seleccionar fresa...", |tool| tool.name.as_str());
            egui::ComboBox::from_label("Fresa de la biblioteca")
                .selected_text(selected).show_ui(ui, |ui| {
                    for tool in library.tools.iter().filter(|tool| supported_spec(tool).is_some()) {
                        ui.selectable_value(&mut self.selected_tool_id, Some(tool.id.clone()),
                            format!("{} · Ø{:.2} mm", tool.name, tool.geometry.diameter_mm()));
                    }
                });
            if previous_tool != self.selected_tool_id { self.clear(); }
            let previous_engine = self.use_sparse;
            ui.checkbox(&mut self.use_sparse, "Motor disperso (resolucion en mm)");
            if previous_engine != self.use_sparse { self.clear(); }
            if self.use_sparse {
                let previous_step = self.spacing_mm;
                egui::ComboBox::from_label("Paso fisico XY")
                    .selected_text(format!("{:.2} mm", self.spacing_mm))
                    .show_ui(ui, |ui| {
                        for step in [0.05, 0.10, 0.20, 0.50] {
                            ui.selectable_value(&mut self.spacing_mm, step, format!("{step:.2} mm"));
                        }
                    });
                if self.spacing_mm != previous_step { self.clear(); }
                ui.small("Sin reduccion automatica de resolucion. Limite grafico: 4,5 millones de vertices; si se supera, se indica el error.");
            }
            let previous_budget = self.history_budget_mib;
            egui::ComboBox::from_label("Limite del historial de RAM")
                .selected_text(format!("{} MiB", self.history_budget_mib))
                .show_ui(ui, |ui| {
                    for mib in [128, 256, 512] {
                        ui.selectable_value(&mut self.history_budget_mib, mib,
                            format!("{} MiB", mib));
                    }
                });
            if previous_budget != self.history_budget_mib { self.clear(); }
            ui.small("El presupuesto afecta al historial de cambios, no a los STL ni a la RAM total del proceso.");
            if !self.use_sparse {
            let previous_resolution = self.resolution;
            egui::ComboBox::from_label("Resolucion de calculo XY")
                .selected_text(format!("{} muestras en el eje mayor", self.resolution))
                .show_ui(ui, |ui| {
                    for resolution in [128, 256, 512, 1024] {
                        ui.selectable_value(&mut self.resolution, resolution,
                            format!("{} · paso aprox. {:.3} mm", resolution,
                                project.stock.as_ref().map_or(0.0, |stock| {
                                    stock.size[0].max(stock.size[1]) / (resolution - 1) as f64
                                })));
                    }
                });
            if previous_resolution != self.resolution { self.clear(); }
            }
            if let Some(job) = &self.job {
                ui.label(format!("Procesando NC: {} / {} movimientos",
                    job.progress.load(Ordering::Relaxed), job.total));
                if ui.button("Cancelar calculo").clicked() {
                    self.clear();
                    self.message = "Simulacion cancelada".into();
                }
            } else if ui.button("Calcular eliminacion de material").clicked()
                && let Err(error) = self.start(project, library, trace)
            {
                self.message = format!("ERROR: {error}");
            }
            if self.field.is_some() || self.sparse_field.is_some() {
                ui.checkbox(&mut self.visible, "Mostrar material restante en visor 3D");
                ui.checkbox(&mut self.isolated_view,
                    "Vista aislada: ocultar STL y trayectorias (mantener punta NC)");
            }
            if let Some(field) = &self.sparse_field {
                let stats = field.stats();
                let [nx, ny] = field.cells();
                ui.monospace(format!("Motor disperso: {nx} x {ny} muestras | paso {:.3} x {:.3} mm",
                    field.spacing_mm()[0], field.spacing_mm()[1]));
                ui.small(format!("Bloques: {} | memoria alturas: {:.1} MiB | checkpoints: {}",
                    stats.tiles, stats.height_bytes as f64 / 1_048_576.0, stats.checkpoints));
                let vertices = nx.checked_mul(ny).unwrap_or(usize::MAX);
                if vertices > 4_500_000 {
                    ui.colored_label(egui::Color32::RED,
                        format!("Superficie demasiado grande para el renderer continuo: {vertices} vertices. No se decima."));
                }
                ui.small("Geometria completa indexada, sombreado por cara; las alturas no se suavizan.");
            }
            if let Some(field) = &self.field {
                ui.monospace(format!("{} x {} muestras · {:.1} mm3 retirados",
                    field.cells[0], field.cells[1], field.removed_mm3()));
                ui.small(format!("Historial RAM: {:.1} / {} MiB · {} cambios · rebobinado disponible",
                    field.history_memory_bytes() as f64 / 1_048_576.0,
                    field.history_budget_mib(), field.history_len()));
                if let Some(snapshot) = &self.snapshot {
                    let step = snapshot.stock.size_mm[0] / (field.cells[0]-1) as f64;
                    if snapshot.tool.geometry.diameter_mm() < step * 2.0 {
                        ui.colored_label(egui::Color32::YELLOW,
                            format!("Fresa estrecha respecto a la malla (paso X {:.3} mm). Aumenta resolucion para mejor detalle.", step));
                    }
                }
                if field.rapid_cut_moves > 0 {
                    ui.colored_label(egui::Color32::YELLOW,
                        format!("AVISO: {} movimientos G0 retiran material con husillo activo.", field.rapid_cut_moves));
                }
                ui.small("El calculo utiliza el texto NC final. El dibujo limita el numero de triangulos para mantener fluidez; la resolucion geometrica se conserva en memoria.");
                ui.small("Superficie intacta: cobre; material retirado: azul. No se calculan huecos internos ni colisiones.");
            }
            if !self.message.is_empty() { ui.label(&self.message); }
        });
    }
}

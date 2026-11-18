//! P2-D2: previsualizacion de material 2.5D a partir del G-code interpretado.
//! Resolucion del calculo independiente del tamano STL; dibujo desacoplado y
//! simplificado para no crear cientos de miles de triangulos egui por frame.

use crate::{
    navigation::Projection,
    project::{Project, cam_coordinates::WorkFrame},
    tools::model::{Tool, ToolGeometry, ToolLibrary},
};
use cam_libre_visor::{
    material_sim::{CutterShape, CutterSpec, MaterialHeightfield, StockBox, segments_from_trace},
    nc_contract::NcTrace,
};
use eframe::egui;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    time::Duration,
};

const MAX_RENDER_NODES: usize = 192;

struct MaterialJob {
    rx: Receiver<Result<MaterialHeightfield, String>>,
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
    visible: bool,
    isolated_view: bool,
    history_budget_mib: usize,
    field: Option<MaterialHeightfield>,
    job: Option<MaterialJob>,
    snapshot: Option<Snapshot>,
    message: String,
}

impl Default for MaterialUi {
    fn default() -> Self {
        Self {
            selected_tool_id: None,
            resolution: 256,
            visible: true,
            isolated_view: true,
            history_budget_mib: 256,
            field: None,
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
    /// Vista aislada solo si existe material calculado y visible.
    /// La herramienta tiene la misma identidad y geometria que la simulacion calculada.
    /// Si el resultado se invalida, no se muestra una fresa obsoleta.
    pub fn simulated_tool(&self) -> Option<&Tool> {
        if !self.visible || self.job.is_some() || self.field.is_none() {
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
        self.isolated_view && self.visible && self.field.is_some() && self.job.is_none()
    }

    pub fn clear(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.field = None;
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
        let stock_spec = snapshot.stock;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_cancel = Arc::clone(&cancel);
        let worker_progress = Arc::clone(&progress);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("lagucam-heightfield".into())
            .spawn(move || {
                let result = MaterialHeightfield::simulate_with_budget(
                    stock_spec,
                    cutter,
                    resolution,
                    budget_mib,
                    &segments,
                    &worker_cancel,
                    &worker_progress,
                );
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
            Some(Ok(Ok(field))) => {
                self.field = Some(field);
                self.job = None;
                self.message = "Simulacion 2.5D preparada. Reproduce o mueve el cursor NC.".into();
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

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        library: &ToolLibrary,
        trace: &NcTrace,
        playhead: f64,
    ) {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|before| self.current_snapshot(project, library).as_ref() != Some(before))
        {
            self.clear();
            self.message = "Fresa o bruto modificado: recalcula el material".into();
        }
        self.poll(ui.ctx());
        if let Some(field) = &mut self.field {
            field.seek(playhead);
        }
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
            if let Some(job) = &self.job {
                ui.label(format!("Procesando NC: {} / {} movimientos",
                    job.progress.load(Ordering::Relaxed), job.total));
                if ui.button("Cancelar calculo").clicked() {
                    self.clear();
                    self.message = "Simulacion cancelada".into();
                }
            } else if ui.button("Calcular eliminacion de material").clicked() {
                if let Err(error) = self.start(project, library, trace) {
                    self.message = format!("ERROR: {error}");
                }
            }
            if let Some(field) = &self.field {
                ui.checkbox(&mut self.visible, "Mostrar material restante en visor 3D");
                ui.checkbox(&mut self.isolated_view,
                    "Vista aislada: ocultar STL y trayectorias (mantener punta NC)");
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

    #[expect(
        clippy::too_many_arguments,
        reason = "Contrato existente de proyeccion del visor"
    )]
    pub fn paint(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        project: &Project,
        _playhead: f64,
        yaw: f32,
        pitch: f32,
        distance: f32,
        pan: glam::Vec3,
        projection: Projection,
    ) {
        if !self.visible || self.job.is_some() {
            return;
        }
        let Some(field) = &self.field else {
            return;
        };
        let Some(model) = &project.model else {
            return;
        };
        // El mesh de egui es 2D (sin depth test frente a GL).
        // Un fondo opaco evita mezclar el STL antiguo y sus trazas con el material.
        if self.isolated_view_active() {
            ui.painter().with_clip_rect(rect).rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgb(159, 207, 232),
            );
        }
        let mvp = crate::navigation::view_projection_mode(
            yaw,
            pitch,
            distance,
            rect.width() / rect.height().max(1.0),
            pan,
            projection,
        );
        let project_xyz = |xyz: [f64; 3]| -> Option<egui::Pos2> {
            let normal = model.visual_point(xyz)?;
            let clip = mvp * normal.extend(1.0);
            if !clip.is_finite() || clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            if !ndc.is_finite() || ndc.z < -1.0 || ndc.z > 1.0 {
                return None;
            }
            Some(
                rect.center()
                    + egui::vec2(ndc.x * rect.width() * 0.5, -ndc.y * rect.height() * 0.5),
            )
        };

        let [nx, ny] = field.cells;
        let rx = nx.min(MAX_RENDER_NODES);
        let ry = ny.min(MAX_RENDER_NODES);
        let xindex = |i: usize| -> usize { i * (nx - 1) / (rx - 1) };
        let yindex = |j: usize| -> usize { j * (ny - 1) / (ry - 1) };
        let xcoord = |ix: usize| {
            field.stock.min_mm[0] + field.stock.size_mm[0] * ix as f64 / (nx - 1) as f64
        };
        let ycoord = |iy: usize| {
            field.stock.min_mm[1] + field.stock.size_mm[1] * iy as f64 / (ny - 1) as f64
        };
        let mut points = Vec::with_capacity(rx * ry);
        let mut colors = Vec::with_capacity(rx * ry);
        for j in 0..ry {
            let iy = yindex(j);
            for i in 0..rx {
                let ix = xindex(i);
                let z = field.height_at(ix, iy);
                points.push(project_xyz([xcoord(ix), ycoord(iy), z]));
                let ix0 = ix.saturating_sub(1);
                let iy0 = iy.saturating_sub(1);
                let ix1 = (ix + 1).min(nx - 1);
                let iy1 = (iy + 1).min(ny - 1);
                let dzdx = (field.height_at(ix1, iy) - field.height_at(ix0, iy))
                    / (xcoord(ix1) - xcoord(ix0)).max(1.0e-9);
                let dzdy = (field.height_at(ix, iy1) - field.height_at(ix, iy0))
                    / (ycoord(iy1) - ycoord(iy0)).max(1.0e-9);
                let shade = ((0.95 - 0.30 * dzdx + 0.23 * dzdy)
                    / (1.0 + dzdx * dzdx + dzdy * dzdy).sqrt())
                .clamp(0.40, 1.15);
                let base = if field.is_cut(ix, iy) {
                    [49.0, 164.0, 202.0]
                } else {
                    [173.0, 134.0, 90.0]
                };
                colors.push(egui::Color32::from_rgb(
                    (base[0] * shade).min(255.0) as u8,
                    (base[1] * shade).min(255.0) as u8,
                    (base[2] * shade).min(255.0) as u8,
                ));
            }
        }
        let mut mesh = egui::Mesh::default();
        // Paredes exteriores con cuatro faldones hasta el fondo real del bruto.
        // Primero se dibujan las paredes; la superficie queda por encima.
        let zbottom = field.stock.min_mm[2];
        let wall_color = egui::Color32::from_rgb(73, 87, 91);
        let mut skirt = |a: (usize, usize), b: (usize, usize)| {
            let a_top = [xcoord(a.0), ycoord(a.1), field.height_at(a.0, a.1)];
            let b_top = [xcoord(b.0), ycoord(b.1), field.height_at(b.0, b.1)];
            let quad = [
                project_xyz(a_top),
                project_xyz(b_top),
                project_xyz([b_top[0], b_top[1], zbottom]),
                project_xyz([a_top[0], a_top[1], zbottom]),
            ];
            if quad.iter().all(Option::is_some) {
                let base = mesh.vertices.len() as u32;
                for point in quad {
                    mesh.colored_vertex(point.expect("Proyeccion verificada"), wall_color);
                }
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base, base + 2, base + 3);
            }
        };
        for i in 0..rx - 1 {
            skirt((xindex(i), 0), (xindex(i + 1), 0));
            skirt((xindex(i + 1), ny - 1), (xindex(i), ny - 1));
        }
        for j in 0..ry - 1 {
            skirt((0, yindex(j + 1)), (0, yindex(j)));
            skirt((nx - 1, yindex(j)), (nx - 1, yindex(j + 1)));
        }
        let base = mesh.vertices.len() as u32;
        for (point, color) in points.iter().zip(colors.iter()) {
            mesh.colored_vertex(point.unwrap_or(egui::Pos2::ZERO), *color);
        }
        for j in 0..ry - 1 {
            for i in 0..rx - 1 {
                let k = j * rx + i;
                if points[k].is_none()
                    || points[k + 1].is_none()
                    || points[k + rx].is_none()
                    || points[k + rx + 1].is_none()
                {
                    continue;
                }
                let a = base + k as u32;
                let b = a + 1;
                let c = a + rx as u32;
                let d = c + 1;
                mesh.add_triangle(a, b, d);
                mesh.add_triangle(a, d, c);
            }
        }
        if !mesh.indices.is_empty() {
            ui.painter()
                .with_clip_rect(rect)
                .add(egui::Shape::mesh(mesh));
        }
    }
}

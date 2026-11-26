//! P2-C2: reproduccion visual del programa NC FINAL, no de PlannedToolpath.
//! El nucleo de interpretacion es P2-C1; aqui solo hay tiempo visual, UI y dibujo.
//! La velocidad es movimientos/segundo, NO tiempo real de la maquina.
//! El cursor se interpola entre endpoints del NC interpretado; no cambia el STL.

use crate::{
    material_ui::MaterialUi,
    navigation::Projection,
    nc_tool_link::{evaluate_tool_link, trace_changes_tool},
    project::{
        Project, cam_coordinates::WorkFrame, model_transform::ModelTransform,
        persistence::sample_identity,
    },
    tools::cam_adapter::ToolSignature,
    tools::model::{Tool, ToolLibrary},
};
use cam_libre_visor::nc_verify::{
    CutterEnvelope, StockEnvelope, VerificationReport, VerificationSeverity, verify_trace,
};
use cam_libre_visor::{
    nc_contract::{
        NcDistanceMode, NcEventKind, NcInitialState, NcMotion, NcMotionKind, NcSpindleMode,
        NcTrace, NcUnits,
    },
    nc_interpreter::interpret_program,
};
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VISIBLE_SEGMENTS: usize = 12_000;
/// Velocidad nominal de referencia para G0; NO describe la maquina real.
const RAPID_ESTIMATE_MM_MIN: f64 = 6_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplayMotion {
    motion: NcMotion,
    line: usize,
}

fn motion_list(trace: &NcTrace) -> Vec<DisplayMotion> {
    trace
        .events
        .iter()
        .filter_map(|event| {
            if let NcEventKind::Motion(motion) = event.kind {
                Some(DisplayMotion {
                    motion,
                    line: event.source_line,
                })
            } else {
                None
            }
        })
        .collect()
}

fn position_at(initial: [f64; 3], motions: &[DisplayMotion], playhead: f64) -> [f64; 3] {
    if motions.is_empty() || playhead <= 0.0 {
        return initial;
    }
    if playhead >= motions.len() as f64 {
        return motions.last().expect("lista no vacia").motion.to_work_mm;
    }
    let i = playhead.floor() as usize;
    let f = playhead - i as f64;
    let m = motions[i].motion;
    std::array::from_fn(|axis| {
        m.from_work_mm[axis] + (m.to_work_mm[axis] - m.from_work_mm[axis]) * f
    })
}

/// Tabla cronologica calculada UNA vez desde el NC ya interpretado.
/// Los offsets preservan el acceso por movimiento del visor y del material 2.5D.
#[derive(Clone, Debug)]
struct MotionTimeline {
    cumulative_secs: Vec<f64>,
}

impl MotionTimeline {
    fn from_motions(motions: &[DisplayMotion]) -> Result<Self, String> {
        let mut cumulative_secs = Vec::with_capacity(motions.len() + 1);
        cumulative_secs.push(0.0);
        for item in motions {
            let motion = item.motion;
            let delta: [f64; 3] =
                std::array::from_fn(|axis| motion.to_work_mm[axis] - motion.from_work_mm[axis]);
            let distance_mm = delta[0].hypot(delta[1]).hypot(delta[2]);
            let feed_mm_min = match motion.kind {
                NcMotionKind::Rapid => RAPID_ESTIMATE_MM_MIN,
                NcMotionKind::LinearFeed => motion.effective_feed_mm_min.ok_or_else(|| {
                    format!(
                        "Linea {}: falta F efectivo para estimar el tiempo",
                        item.line
                    )
                })?,
            };
            if !distance_mm.is_finite() || !feed_mm_min.is_finite() || feed_mm_min <= 0.0 {
                return Err(format!("Linea {}: distancia o avance no valido", item.line));
            }
            let duration = distance_mm * 60.0 / feed_mm_min;
            let next = cumulative_secs.last().copied().unwrap_or(0.0) + duration;
            if !next.is_finite() {
                return Err("La duracion NC supera el rango numerico".into());
            }
            cumulative_secs.push(next);
        }
        Ok(Self { cumulative_secs })
    }

    fn total(&self) -> f64 {
        self.cumulative_secs.last().copied().unwrap_or(0.0)
    }

    fn seconds_at(&self, playhead: f64) -> f64 {
        let n = self.cumulative_secs.len() - 1;
        if n == 0 || playhead <= 0.0 {
            return 0.0;
        }
        if playhead >= n as f64 {
            return self.total();
        }
        let i = playhead.floor() as usize;
        let fraction = playhead - i as f64;
        self.cumulative_secs[i] + (self.cumulative_secs[i + 1] - self.cumulative_secs[i]) * fraction
    }

    fn playhead_at(&self, seconds: f64) -> f64 {
        let n = self.cumulative_secs.len() - 1;
        if n == 0 || seconds <= 0.0 {
            return 0.0;
        }
        if seconds >= self.total() {
            return n as f64;
        }
        let i = self
            .cumulative_secs
            .partition_point(|value| *value <= seconds)
            .saturating_sub(1)
            .min(n - 1);
        let duration = self.cumulative_secs[i + 1] - self.cumulative_secs[i];
        if duration <= 0.0 {
            return i as f64;
        }
        i as f64 + ((seconds - self.cumulative_secs[i]) / duration).clamp(0.0, 1.0)
    }
}

fn estimated_clock(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn next_tick(
    playhead: f64,
    elapsed_secs: f64,
    speed_factor: f64,
    timeline: &MotionTimeline,
) -> f64 {
    // Evitar saltos tras una pausa del proceso o del sistema operativo.
    let delta = elapsed_secs.clamp(0.0, 0.1) * speed_factor;
    let next_seconds = (timeline.seconds_at(playhead) + delta).min(timeline.total());
    timeline.playhead_at(next_seconds)
}

fn validate_source_file(path: &Path) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| format!("No se puede leer el archivo NC: {e}"))?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("El archivo NC no es regular o excede 64 MiB".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Error leyendo NC: {e}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("El archivo NC supera 64 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "El archivo NC no esta codificado como UTF-8".into())
}

/// Identidad de archivo de origen: ruta, metadatos y firma muestreada como CamProvenance.
/// La firma se repite cada 5 segundos, no una vez por frame con STL gigantes.
#[derive(Clone, Debug)]
struct SourceStamp {
    requested_path: PathBuf,
    canonical_path: PathBuf,
    len: u64,
    triangles: u32,
    modified: SystemTime,
    sampled_fnv: String,
}

impl SourceStamp {
    fn capture(path: Option<&Path>, project: &Project) -> Result<Self, String> {
        let path = path.ok_or("No consta el archivo de origen STL en la sesion")?;
        let model = project.model.as_ref().ok_or("No hay modelo STL cargado")?;
        let canonical_path = path
            .canonicalize()
            .map_err(|e| format!("No se puede identificar el STL: {e}"))?;
        let (len, triangles, sampled_fnv, _) = sample_identity(&canonical_path)?;
        if len != model.size_bytes || triangles != model.triangles {
            return Err("El STL ya no coincide con el modelo importado".into());
        }
        let metadata = fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        Ok(Self {
            requested_path: path.to_path_buf(),
            canonical_path,
            len,
            triangles,
            modified,
            sampled_fnv,
        })
    }

    fn metadata_current(&self, selected_path: Option<&Path>) -> bool {
        let Some(path) = selected_path else {
            return false;
        };
        if path != self.requested_path
            && path.canonicalize().ok().as_deref() != Some(self.canonical_path.as_path())
        {
            return false;
        }
        fs::metadata(&self.canonical_path).is_ok_and(|m| {
            m.is_file() && m.len() == self.len && m.modified().is_ok_and(|v| v == self.modified)
        })
    }

    fn samples_current(&self) -> bool {
        sample_identity(&self.canonical_path).is_ok_and(|(len, triangles, signature, _)| {
            len == self.len && triangles == self.triangles && signature == self.sampled_fnv
        })
    }
}

/// Identidad de los datos geometricos y del texto NC que originaron la reproduccion.
#[derive(Clone, Debug)]
struct PlaybackSession {
    source: SourceStamp,
    model_session: u64,
    model_transform: ModelTransform,
    model_name: String,
    model_triangles: u32,
    model_size_bytes: u64,
    model_bounds: crate::stl::Bounds,
    model_raw_bounds: crate::stl::Bounds,
    stock_min: [f64; 3],
    stock_size: [f64; 3],
    nc_text_version: u64,
}

impl PlaybackSession {
    fn capture(
        project: &Project,
        source: Option<&Path>,
        nc_text_version: u64,
    ) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("No hay modelo STL cargado")?;
        let stock = project.stock.as_ref().ok_or("No hay bruto definido")?;
        stock.validate().map_err(str::to_owned)?;
        Ok(Self {
            source: SourceStamp::capture(source, project)?,
            model_session: project.model_session,
            model_transform: project.model_transform,
            model_name: model.name.clone(),
            model_triangles: model.triangles,
            model_size_bytes: model.size_bytes,
            model_bounds: model.bounds,
            model_raw_bounds: model.raw_bounds,
            stock_min: stock.min,
            stock_size: stock.size,
            nc_text_version,
        })
    }

    fn is_current(
        &self,
        project: &Project,
        source_path: Option<&Path>,
        nc_text_version: u64,
    ) -> bool {
        let (Some(model), Some(stock)) = (&project.model, &project.stock) else {
            return false;
        };
        self.model_session == project.model_session
            && self.model_transform == project.model_transform
            && self.model_name == model.name
            && self.model_triangles == model.triangles
            && self.model_size_bytes == model.size_bytes
            && self.model_bounds.min == model.bounds.min
            && self.model_bounds.max == model.bounds.max
            && self.model_raw_bounds.min == model.raw_bounds.min
            && self.model_raw_bounds.max == model.raw_bounds.max
            && self.stock_min == stock.min
            && self.stock_size == stock.size
            && self.nc_text_version == nc_text_version
            && self.source.metadata_current(source_path)
    }
}

struct Playback {
    trace: NcTrace,
    moves: Vec<DisplayMotion>,
    timeline: MotionTimeline,
    frame: WorkFrame,
    identity: PlaybackSession,
}

type VerificationSnapshot = (VerificationReport, Tool, [f64; 3], [f64; 3]);

pub struct NcPlaybackUi {
    pub open: bool,
    compact_generated: bool,
    /// Se incrementa en cada carga de texto NC; el texto no es editable directamente.
    nc_text_version: u64,
    source_path: Option<PathBuf>,
    last_source_check: Option<Instant>,
    nc_text: String,
    source_name: String,
    proposed_start: Option<[f64; 3]>,
    initial_xyz: [f64; 3],
    initial_confirmed: bool,
    playback: Option<Playback>,
    playhead: f64,
    playing: bool,
    speed: f64,
    last_update: Option<Instant>,
    visible: bool,
    show_cutter: bool,
    scene_renderer:
        std::sync::Arc<egui::mutex::Mutex<Option<Result<crate::nc_scene_gl::Renderer, String>>>>,
    material: MaterialUi,
    source_tool: Option<ToolSignature>,
    verify_tool_id: Option<String>,
    verification: Option<VerificationSnapshot>,
    verify_message: String,
    message: String,
}

impl Default for NcPlaybackUi {
    fn default() -> Self {
        Self {
            open: false,
            compact_generated: false,
            nc_text_version: 0,
            source_path: None,
            last_source_check: None,
            nc_text: String::new(),
            source_name: String::new(),
            proposed_start: None,
            initial_xyz: [0.0; 3],
            initial_confirmed: false,
            playback: None,
            playhead: 0.0,
            playing: false,
            speed: 1.0,
            last_update: None,
            visible: true,
            show_cutter: true,
            scene_renderer: std::sync::Arc::new(egui::mutex::Mutex::new(None)),
            material: MaterialUi::default(),
            source_tool: None,
            verify_tool_id: None,
            verification: None,
            verify_message: String::new(),
            message: String::new(),
        }
    }
}

impl NcPlaybackUi {
    /// Solo una sesion compacta realmente preparada bloquea Simular.
    /// La ventana tecnica con texto NC cargado puede ser reemplazada.
    pub fn simulation_open(&self) -> bool {
        self.compact_active()
    }

    pub fn clear(&mut self) {
        self.compact_generated = false;
        self.nc_text_version = self.nc_text_version.wrapping_add(1);
        self.last_source_check = None;
        self.verification = None;
        self.verify_message.clear();
        self.verify_tool_id = None;
        self.source_tool = None;
        self.material.select_source_tool(None);
        self.playback = None;
        self.playing = false;
        self.last_update = None;
        self.playhead = 0.0;
        self.initial_confirmed = false;
        self.proposed_start = None;
        self.nc_text.clear();
        self.source_name.clear();
        self.message.clear();
    }

    /// Validar antes del visor, independientemente de la pestana activa.
    pub fn sync_session(
        &mut self,
        project: &Project,
        library: &ToolLibrary,
        source_path: Option<&Path>,
    ) {
        self.source_path = source_path.map(Path::to_path_buf);
        if self.playback.is_some() && !self.projection_current(project) {
            self.invalidate_derived(
                "STL, transformacion, bruto, origen o NC modificados: vuelve a interpretar",
            );
            return;
        }
        if self.playback.is_some()
            && self
                .last_source_check
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(5))
        {
            self.last_source_check = Some(Instant::now());
            if !self
                .playback
                .as_ref()
                .is_some_and(|pb| pb.identity.source.samples_current())
            {
                self.invalidate_derived(
                    "El archivo STL de origen ha cambiado: vuelve a importarlo y a interpretar NC",
                );
                return;
            }
        }
        self.material.refresh(project, library);
        if self
            .verification
            .as_ref()
            .is_some_and(|(_, tool, min, max)| {
                library.get(&tool.id) != Some(tool)
                    || project
                        .stock
                        .as_ref()
                        .and_then(|stock| stock.max().map(|limit| (stock.min, limit)))
                        != Some((*min, *max))
            })
        {
            self.verification = None;
            self.verify_message =
                "Fresa o bruto modificados: vuelve a verificar el programa NC".into();
        }
    }

    fn invalidate_derived(&mut self, message: &str) {
        self.material.clear(); // Cancela los calculos en segundo plano.
        self.verification = None;
        self.verify_message.clear();
        self.playback = None;
        self.playhead = 0.0;
        self.playing = false;
        self.last_update = None;
        self.last_source_check = None;
        self.message = message.into();
    }

    /// Recibe exactamente la salida textual del postprocesador, no el camino CAM.
    /// La posicion sugerida NO se adopta automaticamente: Start podria mover la herramienta.
    pub fn load_generated(
        &mut self,
        text: String,
        first_cam_position: [f64; 3],
        tool_signature: ToolSignature,
    ) {
        self.clear();
        self.material
            .select_source_tool(Some(&tool_signature.tool_id));
        self.verify_tool_id = Some(tool_signature.tool_id.clone());
        self.source_tool = Some(tool_signature);
        self.nc_text = text;
        self.source_name = "Vista previa NC de LaguCAM".into();
        self.proposed_start = Some(first_cam_position);
        self.open = true;
        self.message =
            "Indica y confirma la posicion real de la herramienta ANTES de Start.".into();
    }

    /// Interpreta el NC generado sin dialogos intermedios. El arranque es
    /// TEORICO: coincide con el inicio geometrico de CAM; NO es un homing
    /// ni confirma la posicion fisica en la maquina.
    pub fn load_generated_compact(
        &mut self,
        text: String,
        first_cam_position: [f64; 3],
        tool_signature: ToolSignature,
        project: &Project,
        library: &ToolLibrary,
    ) -> Result<(), String> {
        if !first_cam_position.iter().all(|v| v.is_finite()) {
            return Err("La posicion inicial CAM contiene valores invalidos".into());
        }
        self.load_generated(text, first_cam_position, tool_signature);
        self.initial_xyz = first_cam_position;
        self.initial_confirmed = true;
        if let Err(error) = self.interpret(project) {
            self.clear();
            self.open = false;
            return Err(format!("No se pudo interpretar el NC generado: {error}"));
        }
        self.compact_generated = true;
        self.visible = true;
        self.show_cutter = false;
        self.message =
            "Simulacion geometrica: posicion inicial teorica CAM; no verifica la maquina".into();
        if let Some(pb) = &self.playback
            && let Err(error) = self.material.prepare_compact(project, library, &pb.trace)
        {
            self.message = format!(
                "NC interpretado. Material 2.5D no disponible: {error}; trayectoria visible"
            );
        }
        Ok(())
    }

    pub fn compact_active(&self) -> bool {
        self.compact_generated && self.open && self.playback.is_some()
    }

    pub fn close_compact(&mut self) {
        self.clear();
        self.open = false;
    }

    /// Controles de reproduccion superpuestos al visor, sin ventana de NC.
    /// Devuelve true cuando el usuario solicita cerrar la simulacion.
    /// Simulacion CAM orientativa: solo se calcula y muestra el resultado final.
    /// La reproduccion NC tecnica se gestiona por separado.
    pub fn compact_controls(
        &mut self,
        ctx: &egui::Context,
        viewport: egui::Rect,
        project: &Project,
        library: &ToolLibrary,
    ) -> bool {
        if !self.compact_active() {
            return false;
        }
        self.playing = false;
        self.last_update = None;
        self.playhead = self.playback.as_ref().map_or(0, |pb| pb.moves.len()) as f64;
        self.material.tick_compact(ctx, project, library, self.playhead);

        let mut close = false;
        let left = (viewport.center().x - 180.0).max(viewport.left() + 8.0);
        let top = (viewport.bottom() - 88.0).max(viewport.top() + 8.0);
        egui::Area::new(egui::Id::unique("lagucam_simulation_controls"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(left, top))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(ui.visuals().window_fill())
                    .stroke(ui.visuals().window_stroke())
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("Resultado final orientativo");
                            if ui.button("Cerrar simulacion").clicked() {
                                close = true;
                            }
                        });
                        ui.small(self.material.compact_status());
                    });
            });
        close
    }
    fn load_file(&mut self) -> Result<(), String> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Programa CNC", &["nc", "gcode", "tap", "txt"])
            .pick_file()
        else {
            return Ok(());
        };
        let text = validate_source_file(&path)?;
        self.clear();
        self.source_name = path.file_name().map_or_else(
            || "Archivo NC".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.nc_text = text;
        self.open = true;
        self.message =
            "Archivo cargado. Introduce la posicion inicial conocida y pulsa Interpretar NC."
                .into();
        Ok(())
    }

    fn initial_state(&self) -> NcInitialState {
        NcInitialState {
            position_work_mm: self.initial_xyz,
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        }
    }

    fn frame(project: &Project) -> Result<WorkFrame, String> {
        let model = project
            .model
            .as_ref()
            .ok_or("Abre un modelo STL para proyectar el NC en 3D")?;
        let stock = project
            .stock
            .as_ref()
            .ok_or("Define un bruto para proyectar el NC")?;
        WorkFrame::new(&project.origin, stock, &model.bounds)
            .map_err(|e| format!("Origen de trabajo no admitido: {e}"))
    }

    fn interpret(&mut self, project: &Project) -> Result<(), String> {
        self.verification = None;
        self.verify_message.clear();
        self.material.clear();
        if self.nc_text.is_empty() {
            return Err("Selecciona un archivo NC o abrelo desde Postprocesar NC".into());
        }
        if !self.initial_confirmed {
            return Err("Debes confirmar expresamente la posicion inicial ANTES de Start".into());
        }
        let frame = Self::frame(project)?;
        let trace =
            interpret_program(&self.nc_text, self.initial_state()).map_err(|e| e.to_string())?;
        let moves = motion_list(&trace);
        let timeline = MotionTimeline::from_motions(&moves)?;
        let identity =
            PlaybackSession::capture(project, self.source_path.as_deref(), self.nc_text_version)?;
        self.playback = Some(Playback {
            trace,
            moves,
            timeline,
            frame,
            identity,
        });
        self.last_source_check = Some(Instant::now());
        self.playing = false;
        self.last_update = None;
        self.playhead = 0.0;
        Ok(())
    }

    fn projection_current(&self, project: &Project) -> bool {
        let Some(playback) = &self.playback else {
            return false;
        };
        playback
            .identity
            .is_current(project, self.source_path.as_deref(), self.nc_text_version)
            && Self::frame(project).is_ok_and(|frame| frame == playback.frame)
    }

    /// Ventana flotante anterior, preservada y apoyada en el contenido compartido.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        tool_library: &crate::tools::model::ToolLibrary,
    ) {
        if !self.open {
            self.playing = false;
            self.last_update = None;
            return;
        }
        let mut open = self.open;
        egui::Window::new("Reproduccion NC (G-code interpretado)")
            .open(&mut open)
            .default_size([570.0, 510.0])
            .min_width(460.0)
            .resizable(true)
            .show(ctx, |ui| self.show_embedded(ui, project, tool_library));
        self.open = open;
    }

    /// Una sola implementacion del reproductor y del verificador P2-E1.
    fn show_embedded(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        tool_library: &crate::tools::model::ToolLibrary,
    ) {
        let ctx = ui.ctx().clone();
        if self.playback.is_some() && !self.projection_current(project) {
            self.invalidate_derived(
                "STL, transformacion, bruto, origen o NC modificados: vuelve a interpretar",
            );
        }
        let now = Instant::now();
        if self.playing
            && let Some(pb) = &self.playback
        {
            let secs = self.last_update.map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f64()
            });
            self.playhead = next_tick(self.playhead, secs, self.speed, &pb.timeline);
            if self.playhead >= pb.moves.len() as f64 {
                self.playing = false;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        self.last_update = Some(now);
        let mut load_file = false;
        let mut interpret = false;
        let mut copy_proposal = false;
        let mut change_initial = false;
        let mut verify_requested = false;

        {
            ui.horizontal(|ui| {
                if ui.button("Abrir archivo NC...").clicked() {
                    load_file = true;
                }
                ui.label(&self.source_name);
            });
            ui.colored_label(egui::Color32::YELLOW,
                    "Reproduccion geometrica; NO verifica colisiones, aceleraciones ni seguridad de maquina.");
            ui.separator();
            ui.label("Posicion inicial de la punta de herramienta, en coordenadas de trabajo (mm), ANTES de Start:");
            ui.horizontal(|ui| {
                for axis in 0..3 {
                    ui.label(["X", "Y", "Z"][axis]);
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.initial_xyz[axis])
                                .speed(0.1)
                                .max_decimals(6),
                        )
                        .changed()
                    {
                        change_initial = true;
                    }
                }
            });
            if let Some(suggested) = self.proposed_start {
                ui.horizontal_wrapped(|ui| {
                        if ui.button("Copiar posicion inicial CAM propuesta").clicked() {
                            copy_proposal = true;
                        }
                        ui.small(format!("X{:.3} Y{:.3} Z{:.3} mm; puede ser incorrecta si Start contiene movimientos.",
                            suggested[0], suggested[1], suggested[2]));
                    });
            }
            if ui
                .checkbox(
                    &mut self.initial_confirmed,
                    "He comprobado la posicion inicial y entiendo que no se deduce del G-code",
                )
                .changed()
            {
                change_initial = true;
            }
            if ui.button("Interpretar NC y preparar trayectoria").clicked() {
                interpret = true;
            }
            if let Some(pb) = &self.playback {
                ui.separator();
                ui.label(format!(
                    "{} movimientos NC · {} eventos interpretados · {} lineas NC",
                    pb.moves.len(),
                    pb.trace.events.len(),
                    self.nc_text.lines().count()
                ));
                if pb.moves.is_empty() {
                    ui.label("El archivo no contiene desplazamientos XYZ.");
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .button(if self.playing { "Pausar" } else { "Reproducir" })
                            .clicked()
                        {
                            self.playing = !self.playing;
                            self.last_update = None;
                            ctx.request_repaint();
                        }
                        if ui.button("Reiniciar").clicked() {
                            self.playhead = 0.0;
                            self.playing = false;
                        }
                        if ui.button("Anterior").clicked() {
                            self.playing = false;
                            self.playhead = (self.playhead.ceil() - 1.0).max(0.0);
                        }
                        if ui.button("Siguiente").clicked() {
                            self.playing = false;
                            self.playhead =
                                (self.playhead.floor() + 1.0).min(pb.moves.len() as f64);
                        }
                    });
                    ui.add(
                        egui::Slider::new(&mut self.playhead, 0.0..=pb.moves.len() as f64)
                            .text("Movimiento NC"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.speed, 0.1..=20.0)
                            .logarithmic(true)
                            .text("Velocidad x (tiempo estimado)"),
                    );
                    ui.small(format!(
                        "Tiempo estimado {} / {}  ·  G0 ref. {:.0} mm/min  ·  Sin aceleraciones",
                        estimated_clock(pb.timeline.seconds_at(self.playhead)),
                        estimated_clock(pb.timeline.total()),
                        RAPID_ESTIMATE_MM_MIN,
                    ));
                    let i = self.playhead.floor() as usize;
                    let line = pb
                        .moves
                        .get(i)
                        .or_else(|| pb.moves.last())
                        .map_or(0, |m| m.line);
                    let xyz =
                        position_at(pb.trace.initial.position_work_mm, &pb.moves, self.playhead);
                    ui.monospace(format!(
                        "Linea NC: {line}  ·  X{:.4} Y{:.4} Z{:.4} mm",
                        xyz[0], xyz[1], xyz[2]
                    ));
                }
                ui.checkbox(&mut self.visible, "Mostrar trayectoria NC en visor 3D");
                ui.checkbox(
                    &mut self.show_cutter,
                    "Mostrar fresa 3D (herramienta de simulacion)",
                );
                ui.small("La fresa se superpone al material: aun no hay ocultacion por profundidad ni control de colisiones.");
                ui.small("G0 en cian; G1 en naranja. El marcador representa la punta, no la geometria completa de la fresa.");
                if pb.moves.len() > MAX_VISIBLE_SEGMENTS {
                    ui.small("Vista de trazos simplificada por rendimiento; reproduccion y posicion conservan todos los movimientos.");
                }
            }
            if let Some(pb) = &self.playback {
                self.material
                    .show(ui, project, tool_library, &pb.trace, self.playhead);
                let selected_tool = self
                    .material
                    .selected_tool_id()
                    .and_then(|id| tool_library.get(id));
                let link = evaluate_tool_link(
                    self.source_tool.as_ref(),
                    selected_tool,
                    trace_changes_tool(&pb.trace),
                );
                ui.colored_label(
                    link.color(),
                    format!("Fresa de simulacion: {}", link.label()),
                );
            }
        }
        if self.playback.is_some() {
            ui.separator();
            egui::CollapsingHeader::new("Verificacion preliminar del programa NC")
                        .default_open(true)
                        .show(ui, |ui| {
                        ui.small("Comprueba el G-code interpretado frente al bruto ORIGINAL. G0 = posible contacto; no confirma colision ni material remanente.");
                        let simulated_id = self.material.simulated_tool().map(|tool| tool.id.clone());
                        let mut chosen = self.verify_tool_id.clone().or(simulated_id);
                        let tool_name = chosen.as_deref().and_then(|id| tool_library.get(id))
                            .map_or("Seleccionar fresa plana o esferica".to_owned(), |tool| tool.name.clone());
                        egui::ComboBox::from_label("Fresa para verificacion")
                            .selected_text(tool_name)
                            .show_ui(ui, |ui| {
                                for tool in &tool_library.tools {
                                    if matches!(&tool.geometry,
                                        crate::tools::model::ToolGeometry::Flat { .. }
                                            | crate::tools::model::ToolGeometry::BallNose { .. }) {
                                        ui.selectable_value(&mut chosen, Some(tool.id.clone()), &tool.name);
                                    }
                                }
                            });
                        let selected_tool = chosen.as_deref().and_then(|id| tool_library.get(id));
                        let link = evaluate_tool_link(
                            self.source_tool.as_ref(), selected_tool, self.playback.as_ref().is_some_and(|pb| trace_changes_tool(&pb.trace))
                        );
                        ui.colored_label(link.color(), format!(
                            "Fresa de verificacion: {}", link.label()
                        ));
                        if chosen != self.verify_tool_id {
                            self.verify_tool_id = chosen;
                            self.verification = None;
                        }
                        if ui.button("Verificar movimientos del G-code").clicked() {
                            verify_requested = true;
                        }
                        if let Some((report, verified_tool, stock_min, stock_max)) = &self.verification
                            && tool_library.get(&verified_tool.id) == Some(verified_tool)
                            && project.stock.as_ref().and_then(|stock| stock.max().map(|max| (stock.min, max)))
                                .is_some_and(|(min, max)| min == *stock_min && max == *stock_max)
                        {
                            ui.label(format!("{} movimientos examinados · {} avisos ({} mostrados)",
                                report.checked_moves, report.total_issues(), report.issues.len()));
                            ui.label(format!("G0 con posible contacto: {} · G1 bajo la base: {} · G1 fuera de XY: {}",
                                report.rapid_stock_candidates, report.feed_below_base, report.feed_outside_xy));
                            ui.label(format!("G1 con husillo parado: {} · comandos T no asociados: {}",
                                report.feed_spindle_stopped, report.unverified_tool_commands));
                            if report.omitted_issues > 0 {
                                ui.colored_label(egui::Color32::YELLOW,
                                    format!("{} avisos adicionales no mostrados; recuentos completos conservados", report.omitted_issues));
                            }
                            egui::ScrollArea::vertical().max_height(185.0).show(ui, |ui| {
                                for issue in &report.issues {
                                    let caption = format!("L{} · {}", issue.source_line, issue.code.label());
                                    let color = if issue.severity == VerificationSeverity::Warning {
                                        egui::Color32::YELLOW
                                    } else {
                                        ui.visuals().text_color()
                                    };
                                    if ui.add(egui::Button::new(egui::RichText::new(caption).color(color))).clicked() {
                                        let Some(index) = issue.move_index else { continue; };
                                            self.playhead = index as f64;
                                            self.playing = false;
                                            self.last_update = None;
                                            ui.ctx().request_repaint();
                                    }
                                }
                            });
                        }
                        if !self.verify_message.is_empty() {
                            ui.colored_label(egui::Color32::YELLOW, &self.verify_message);
                        }
                    });
        }
        if !self.message.is_empty() {
            ui.separator();
            ui.label(&self.message);
        }
        if copy_proposal && let Some(suggested) = self.proposed_start {
            self.initial_xyz = suggested;
            self.initial_confirmed = false;
            change_initial = true;
        }
        if change_initial {
            self.verification = None;
            self.playing = false;
            self.last_update = None;
            self.playback = None;
            self.material.clear();
        }
        if load_file && let Err(e) = self.load_file() {
            self.message = format!("ERROR: {e}");
        }
        if interpret {
            match self.interpret(project) {
                Ok(()) => self.message = "NC interpretado sin instrucciones desconocidas. No es una validacion fisica de la maquina.".into(),
                Err(e) => {
                    self.playback = None;
                    self.playing = false;
                    self.message = format!("ERROR: {e}");
                }
            }
        }
        if verify_requested {
            let result = (|| -> Result<VerificationSnapshot, String> {
                let playback = self
                    .playback
                    .as_ref()
                    .ok_or("Interpreta primero el programa NC")?;
                let model = project.model.as_ref().ok_or("No hay STL abierto")?;
                let stock = project.stock.as_ref().ok_or("No hay bruto definido")?;
                let tool_id = self
                    .verify_tool_id
                    .as_deref()
                    .ok_or("Selecciona una fresa plana o esferica de la biblioteca")?;
                let tool = tool_library
                    .get(tool_id)
                    .ok_or("La fresa seleccionada ya no existe")?;
                tool.validate()?;
                if !matches!(
                    &tool.geometry,
                    crate::tools::model::ToolGeometry::Flat { .. }
                        | crate::tools::model::ToolGeometry::BallNose { .. }
                ) {
                    return Err(
                        "La verificacion admite por ahora solo fresas planas y esfericas".into(),
                    );
                }
                let max = stock.max().ok_or("Limites del bruto invalidos")?;
                let report = verify_trace(
                    &playback.trace,
                    StockEnvelope {
                        min_mm: stock.min,
                        max_mm: max,
                    },
                    CutterEnvelope {
                        radius_mm: tool.geometry.diameter_mm() * 0.5,
                        cutting_length_mm: tool.cutting_length_mm,
                    },
                    |point| {
                        project
                            .origin
                            .work_to_model(stock, &model.bounds, point)
                            .map_err(str::to_owned)
                    },
                )?;
                Ok((report, tool.clone(), stock.min, max))
            })();
            match result {
                Ok(report) => {
                    self.verification = Some(report);
                    self.verify_message = "Analisis conservador completado. No es una certificacion de seguridad CNC.".into();
                }
                Err(error) => {
                    self.verification = None;
                    self.verify_message = format!("ERROR verificando NC: {error}");
                }
            }
        }
    }

    /// Liberar los recursos GL en el hilo de la ventana, antes de cerrar contexto.
    pub fn destroy_scene_gl(&mut self, gl: &eframe::glow::Context) {
        let mut renderer = self.scene_renderer.lock();
        if let Some(Ok(renderer)) = renderer.take() {
            renderer.destroy(gl);
        }
    }
    /// Pinta un overlay de la traza NC. El modulo no consulta CAM ni ContactSolver.
    #[expect(
        clippy::too_many_arguments,
        reason = "Contrato de proyeccion del visor 3D existente"
    )]
    pub fn paint(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        project: &Project,
        yaw: f32,
        pitch: f32,
        distance: f32,
        pan: glam::Vec3,
        projection: Projection,
    ) {
        if !self.open || !self.projection_current(project) {
            return;
        }
        let Some(pb) = &self.playback else {
            return;
        };
        let Some(model) = &project.model else {
            return;
        };
        let Some(stock) = &project.stock else {
            return;
        };
        let mvp = crate::navigation::view_projection_mode(
            yaw,
            pitch,
            distance,
            rect.width() / rect.height().max(1.0),
            pan,
            projection,
        );
        let point = |work_xyz: [f64; 3]| -> Option<egui::Pos2> {
            let model_xyz = project
                .origin
                .work_to_model(stock, &model.bounds, work_xyz)
                .ok()?;
            let pos = model.visual_point(model_xyz)?;
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
        // P2-D3: todos los solidos de la simulacion usan la MISMA profundidad GL.
        // El modelo STL original queda fuera de la vista de simulacion. Nunca se
        // triangula material en pantalla (Pos2) ni se ordenan las caras por Z media.
        let sparse_scene = self
            .material
            .render_sparse_surface(project)
            .ok()
            .flatten()
            .map(crate::nc_scene_gl::Scene::from_sparse_surface);
        let dense_scene = self
            .material
            .render_field()
            .and_then(|field| crate::nc_scene_gl::Scene::from_material(project, field));
        if let Some(mut scene) = sparse_scene
            .or(dense_scene)
            .or_else(|| crate::nc_scene_gl::Scene::from_stock(project))
        {
            let tip = position_at(
                pb.trace.initial.position_work_mm,
                &pb.moves,
                self.material.visual_cursor(self.playhead),
            );
            if self.show_cutter
                && let Some(tool) = self.material.simulated_tool()
            {
                scene.append_tool(project, tool, tip);
            }
            if self.visible {
                let stride = pb.moves.len().div_ceil(MAX_VISIBLE_SEGMENTS).max(1);
                if let (Some(model), Some(stock)) = (&project.model, &project.stock) {
                    for item in pb.moves.iter().step_by(stride) {
                        let a = project.origin.work_to_model(
                            stock,
                            &model.bounds,
                            item.motion.from_work_mm,
                        );
                        let b = project.origin.work_to_model(
                            stock,
                            &model.bounds,
                            item.motion.to_work_mm,
                        );
                        if let (Ok(a), Ok(b)) = (a, b) {
                            let color = match item.motion.kind {
                                NcMotionKind::Rapid => [0.18, 0.80, 0.96],
                                NcMotionKind::LinearFeed => [1.0, 0.58, 0.11],
                            };
                            scene.add_line(project, a, b, color);
                        }
                    }
                }
            }
            let scene_renderer = std::sync::Arc::clone(&self.scene_renderer);
            let camera = crate::stock_render::Camera {
                yaw,
                pitch,
                zoom: distance,
                aspect: rect.width() / rect.height().max(1.0),
                pan,
                projection,
            };
            ui.painter().with_clip_rect(rect).add(egui::PaintCallback {
                rect,
                callback: std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(
                    move |info, painter| {
                        use eframe::glow::HasContext as _;
                        let gl = painter.gl();
                        let clip = egui::PaintCallbackInfo {
                            clip_rect: info.clip_rect.intersect(info.viewport),
                            ..info
                        }
                        .clip_rect_in_pixels();
                        unsafe {
                            gl.enable(eframe::glow::SCISSOR_TEST);
                            gl.scissor(
                                clip.left_px,
                                clip.from_bottom_px,
                                clip.width_px,
                                clip.height_px,
                            );
                            // Apartar el STL y dibujar fondo+material en el mismo FBO.
                            gl.clear_color(0.62, 0.81, 0.91, 1.0);
                            gl.depth_mask(true);
                            gl.clear(
                                eframe::glow::COLOR_BUFFER_BIT | eframe::glow::DEPTH_BUFFER_BIT,
                            );
                        }
                        let mut cache = scene_renderer.lock();
                        if cache.is_none() {
                            *cache = Some(crate::nc_scene_gl::Renderer::new(gl));
                            if let Some(Err(error)) = cache.as_ref() {
                                eprintln!("LaguCAM P2-D3: {}", error);
                            }
                        }
                        if let Some(Ok(renderer)) = cache.as_mut() {
                            renderer.paint(gl, &scene, camera);
                        }
                    },
                )),
            });
            return;
        }
        if !self.visible
            && !self.material.isolated_view_active()
            && !(self.show_cutter && self.material.simulated_tool().is_some())
        {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        if !self.material.isolated_view_active() {
            let stride = pb.moves.len().div_ceil(MAX_VISIBLE_SEGMENTS).max(1);
            for (i, item) in pb.moves.iter().enumerate().step_by(stride) {
                let Some(a) = point(item.motion.from_work_mm) else {
                    continue;
                };
                let Some(b) = point(item.motion.to_work_mm) else {
                    continue;
                };
                let played = i as f64 <= self.playhead;
                let color = match (item.motion.kind, played) {
                    (NcMotionKind::Rapid, true) => egui::Color32::from_rgb(55, 210, 245),
                    (NcMotionKind::LinearFeed, true) => egui::Color32::from_rgb(245, 163, 64),
                    (NcMotionKind::Rapid, false) => egui::Color32::from_rgb(48, 111, 136),
                    (NcMotionKind::LinearFeed, false) => egui::Color32::from_rgb(155, 107, 58),
                };
                painter.line_segment([a, b], egui::Stroke::new(1.2, color));
            }
            let index = self.playhead.floor() as usize;
            if let Some(current) = pb.moves.get(index)
                && let (Some(a), Some(b)) = (
                    point(current.motion.from_work_mm),
                    point(current.motion.to_work_mm),
                )
            {
                painter.line_segment([a, b], egui::Stroke::new(2.8, egui::Color32::WHITE));
            }
        }
        let tip = position_at(
            pb.trace.initial.position_work_mm,
            &pb.moves,
            self.material.visual_cursor(self.playhead),
        );
        if self.show_cutter
            && let Some(tool) = self.material.simulated_tool()
        {
            crate::nc_tool_overlay::paint(
                ui,
                rect,
                project,
                tool,
                tip,
                crate::nc_tool_overlay::Camera {
                    yaw,
                    pitch,
                    distance,
                    pan,
                    projection,
                },
            );
        }
        if let Some(pos) = point(tip) {
            painter.circle_filled(pos, 6.0, egui::Color32::from_rgb(235, 55, 62));
            painter.circle_stroke(pos, 8.0, egui::Stroke::new(1.6, egui::Color32::WHITE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        project::{model::ModelReference, stock::Stock},
        stl::Bounds,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn technical_nc_preview_does_not_block_compact_simulation() {
        let mut playback = NcPlaybackUi {
            open: true,
            nc_text: "G21\nM30\n".into(),
            ..Default::default()
        };
        assert!(!playback.simulation_open());
        // Sin interpretar el NC, tampoco hay sesion compacta reproducible.
        playback.compact_generated = true;
        assert!(!playback.simulation_open());
        playback.close_compact();
        assert!(!playback.simulation_open());
    }

    #[test]
    fn compact_without_program_is_closed() {
        let mut playback = NcPlaybackUi::default();
        assert!(!playback.compact_active());
        playback.close_compact();
        assert!(!playback.open);
        assert!(!playback.compact_active());
    }

    #[test]
    fn rotation_with_identical_bounds_invalidates_nc_and_material() {
        let path = std::env::temp_dir().join(format!(
            "lagucam_nc_session_{}_{}.stl",
            std::process::id(),
            NEXT_TMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut bytes = vec![0u8; 134];
        bytes[80..84].copy_from_slice(&1u32.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        let bounds = Bounds {
            min: [-2.0, -2.0, 0.0],
            max: [2.0, 2.0, 4.0],
        };
        let mut project = Project {
            model_session: 7,
            model: Some(ModelReference {
                name: "symmetric.stl".into(),
                bounds,
                raw_bounds: bounds,
                triangles: 1,
                size_bytes: 134,
            }),
            stock: Some(Stock::from_bounds(&bounds)),
            ..Project::default()
        };
        let mut player = NcPlaybackUi {
            source_path: Some(path.clone()),
            nc_text: "G21\nG90\nG94\nS8000 M3\nG0 X1 Y0 Z5\nF600 G1 Z0\nM5\nM30\n".into(),
            initial_xyz: [0.0, 0.0, 5.0],
            initial_confirmed: true,
            ..NcPlaybackUi::default()
        };
        player.interpret(&project).unwrap();
        player.open = true;
        assert!(!player.simulation_open());
        player.compact_generated = true;
        assert!(player.simulation_open());
        player.compact_generated = false;
        assert!(player.projection_current(&project));

        let initial_bounds = project.model.as_ref().unwrap().bounds;
        let mut rotated = project.model_transform;
        rotated.rotation_degrees[2] = 180.0;
        // Construir explicitamente el peor caso: distinto transform, mismo AABB bit a bit.
        project.model_transform = rotated;
        assert_eq!(
            initial_bounds.min,
            project.model.as_ref().unwrap().bounds.min
        );
        assert_eq!(
            initial_bounds.max,
            project.model.as_ref().unwrap().bounds.max
        );
        assert!(!player.projection_current(&project));

        project.model_transform = ModelTransform::default();
        assert!(player.projection_current(&project));
        project.model_session += 1;
        assert!(!player.projection_current(&project));
        project.model_session -= 1;
        assert!(player.projection_current(&project));
        player.nc_text_version += 1;
        assert!(!player.projection_current(&project));
        fs::remove_file(path).unwrap();
    }

    fn simple() -> NcTrace {
        interpret_program(
            "G21\nG90\nG94\nS8000 M3\nG0 X10 Y0 Z5\nF600 G1 Z0\nM5\nM30\n",
            NcInitialState {
                position_work_mm: [0.0, 0.0, 5.0],
                units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute,
                feed_mm_min: None,
                spindle_rpm: None,
                spindle_mode: NcSpindleMode::Stopped,
                tool_number: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn motion_list_comes_from_nc_events_and_retains_source_lines() {
        let list = motion_list(&simple());
        assert_eq!(list.len(), 2);
        assert_eq!([list[0].line, list[1].line], [5, 6]);
        assert_eq!(list[0].motion.kind, NcMotionKind::Rapid);
        assert_eq!(list[1].motion.kind, NcMotionKind::LinearFeed);
    }

    #[test]
    fn cursor_interpolates_nc_and_never_invents_an_initial_move() {
        let trace = simple();
        let list = motion_list(&trace);
        assert_eq!(
            position_at(trace.initial.position_work_mm, &list, 0.0),
            [0.0, 0.0, 5.0]
        );
        assert_eq!(
            position_at(trace.initial.position_work_mm, &list, 0.5),
            [5.0, 0.0, 5.0]
        );
        assert_eq!(
            position_at(trace.initial.position_work_mm, &list, 1.5),
            [10.0, 0.0, 2.5]
        );
        assert_eq!(
            position_at(trace.initial.position_work_mm, &list, 2.0),
            [10.0, 0.0, 0.0]
        );
    }

    #[test]
    fn playback_time_uses_nc_feed_and_rapid_reference() {
        let motions = motion_list(&simple());
        let timeline = MotionTimeline::from_motions(&motions).unwrap();
        // G0 10 mm a 6000 mm/min = 0.1 s.
        // G1 5 mm a F600 mm/min = 0.5 s.
        assert!((timeline.total() - 0.6).abs() < 1e-12);
        assert!((timeline.seconds_at(1.5) - 0.35).abs() < 1e-12);
        assert!((timeline.playhead_at(0.35) - 1.5).abs() < 1e-12);
        assert!((next_tick(0.0, 0.01, 1.0, &timeline) - 0.1).abs() < 1e-12);
        assert_eq!(next_tick(0.0, 600.0, 1.0, &timeline), 1.0);
        assert_eq!(next_tick(1.9, 1.0, 20.0, &timeline), 2.0);
    }

    #[test]
    fn timeline_preserves_short_and_long_moves_with_different_feed() {
        let trace = interpret_program(
            "G21 G90 G94\nF60 G1 X1\nF600 G1 X101\nM30\n",
            NcInitialState {
                position_work_mm: [0.0; 3],
                units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute,
                feed_mm_min: None,
                spindle_rpm: None,
                spindle_mode: NcSpindleMode::Stopped,
                tool_number: None,
            },
        )
        .unwrap();
        let timing = MotionTimeline::from_motions(&motion_list(&trace)).unwrap();
        assert!((timing.total() - 11.0).abs() < 1e-9);
        assert!((timing.playhead_at(1.0) - 1.0).abs() < 1e-9);
        assert!((timing.playhead_at(6.0) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn code_with_unknown_instruction_does_not_create_playable_trace() {
        let result = interpret_program(
            "G21\nG54\nM30\n",
            NcInitialState {
                position_work_mm: [0.0; 3],
                units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute,
                feed_mm_min: None,
                spindle_rpm: None,
                spindle_mode: NcSpindleMode::Stopped,
                tool_number: None,
            },
        );
        assert!(result.is_err());
    }
}

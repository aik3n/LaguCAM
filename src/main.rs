#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cam_batch;
mod cam_combined_nc;
mod cam_markers_3d;
mod machine_ui;
mod material_ui;
mod nav_cube;
mod navigation;
mod nc_export_ui;
mod nc_playback_ui;
mod nc_scene_gl;
mod nc_tool_link;
mod nc_tool_overlay;
mod options;
mod overlay;
mod panel_layout;
mod portable;
mod post_ui;
mod preview;
mod project;
mod project_ui;
mod recent_projects;
mod render;
mod sample_direct;
mod sample_overlay;
mod scene_pick;
mod section_overlay;
mod stl;
mod stock_render;
mod test_parallel;
mod tools;
mod tools_profile;
mod tools_ui;
mod work_units;

use eframe::{egui, egui_glow, glow};
use egui::mutex::Mutex;
use preview::LoadEvent;
use project::persistence;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

struct PendingSuccess {
    info: stl::StlInfo,
    time_seconds: f32,
}

struct OpeningProject {
    document: persistence::ProjectDocument,
    path: PathBuf,
    relocated: bool,
    warning: Option<String>,
}

#[derive(Clone)]
enum FileAction {
    New,
    OpenProject,
    OpenRecent(PathBuf),
    OpenStl,
    Exit,
}

type PickResult = Result<Option<glam::Vec3>, String>;
type PickMailbox = Arc<Mutex<Option<PickResult>>>;

/// El clic se procesa en el callback GL. Un arrastre que llegue antes del
/// resultado se acumula y se ejecuta solo cuando se conoce el nuevo pivote.
/// Este estado pertenece a la UI, no a un formato de geometria.
#[derive(Default)]
struct PivotPress {
    active: bool,
    waiting_for_hit: bool,
    deferred_drag: egui::Vec2,
}

impl PivotPress {
    fn start(&mut self) {
        self.active = true;
        self.waiting_for_hit = true;
        self.deferred_drag = egui::Vec2::ZERO;
    }

    /// La liberacion oculta la marca; no elimina el pivote seleccionado.
    /// Tampoco cancela una consulta GPU pendiente de resultado.
    fn sync_held(&mut self, held: bool) {
        if !held {
            self.active = false;
        }
    }

    fn marker_visible(&self) -> bool {
        self.active && !self.waiting_for_hit
    }

    fn motion(&mut self, delta: egui::Vec2) -> Option<egui::Vec2> {
        if self.waiting_for_hit {
            self.deferred_drag += delta;
            None
        } else {
            Some(delta)
        }
    }

    fn complete(&mut self) -> egui::Vec2 {
        self.waiting_for_hit = false;
        std::mem::take(&mut self.deferred_drag)
    }
}

/// Asigna acciones de navegacion a botones. No depende del tipo de modelo.
/// Las asignaciones son iniciales; una futura UI podra cambiarlas sin reescribir
/// la orbita, el zoom o el selector de superficies.
#[derive(Clone, Copy)]
struct NavigationBindings {
    orbit: egui::PointerButton,
    pan: [egui::PointerButton; 2],
}

impl Default for NavigationBindings {
    fn default() -> Self {
        Self {
            orbit: egui::PointerButton::Primary,
            pan: [egui::PointerButton::Secondary, egui::PointerButton::Middle],
        }
    }
}

impl NavigationBindings {
    fn orbit_down(self, ctx: &egui::Context) -> bool {
        ctx.input(|i| i.pointer.button_down(self.orbit))
    }

    fn orbit_pressed(self, ctx: &egui::Context) -> bool {
        ctx.input(|i| i.pointer.button_pressed(self.orbit))
    }

    fn orbit_dragged(self, response: &egui::Response) -> bool {
        response.dragged_by(self.orbit)
    }

    fn pan_dragged(self, response: &egui::Response) -> bool {
        self.pan.iter().any(|button| response.dragged_by(*button))
    }
}

struct App {
    renderer: Arc<Mutex<render::Render3d>>,
    stock_renderer: Arc<Mutex<stock_render::StockRenderer>>,
    cam_marker_renderer: Arc<Mutex<cam_markers_3d::MarkerRenderer3d>>,
    project: project::Project,
    property_error: String,
    sample_profiles: Option<sample_overlay::Profiles>,
    section_profiles: Option<section_overlay::Sections>,
    sample_profile_job: Option<sample_direct::ProfileJob>,
    tool_library: tools::model::ToolLibrary,
    tool_library_path: Option<PathBuf>,
    tool_ui: tools_ui::ToolUi,
    post_ui: post_ui::PostUi,
    machine_ui: machine_ui::MachineUi,
    nc_export_ui: nc_export_ui::NcExportUi,
    nc_batch_export: cam_combined_nc::NcBatchExportUi,
    nc_playback_ui: nc_playback_ui::NcPlaybackUi,
    test_parallel: test_parallel::TestParallelPanel,
    pending_cam_command: Option<project_ui::OperationCommand>,
    batch_calculation: Option<cam_batch::BatchCalculation>,
    project_path: Option<PathBuf>,
    recent_projects: recent_projects::RecentProjects,
    current_stl_path: Option<PathBuf>,
    pending_stl_path: Option<PathBuf>,
    opening_project: Option<OpeningProject>,
    saved_state: Option<persistence::ProjectState>,
    pending_action: Option<FileAction>,
    ask_unsaved: bool,
    exit_approved: bool,
    receiving: Option<Receiver<LoadEvent>>,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    pending_filename: Option<String>,
    pending_success: Option<PendingSuccess>,
    filename: String,
    message: String,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: glam::Vec3,
    projection: navigation::Projection,
    pending_pick: Option<egui::Pos2>,
    pick_result: PickMailbox,
    orbit_pivot: Option<glam::Vec3>,
    pivot_press: PivotPress,
    navigation_bindings: NavigationBindings,
    panels: panel_layout::PanelLayout,
    options: options::Options,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let gl = cc.gl.as_ref().ok_or("Se necesita OpenGL")?;
        let mut mesh_renderer = render::Render3d::new(gl)?;
        let stock_renderer = match stock_render::StockRenderer::new(gl) {
            Ok(renderer) => renderer,
            Err(error) => {
                mesh_renderer.destroy(gl);
                return Err(error);
            }
        };
        let marker_renderer = match cam_markers_3d::MarkerRenderer3d::new(gl) {
            Ok(renderer) => renderer,
            Err(error) => {
                stock_renderer.destroy(gl);
                mesh_renderer.destroy(gl);
                return Err(error);
            }
        };
        // Biblioteca independiente de proyectos STL y ficheros .lagucam.
        let (tool_library, tool_library_path, tool_error) = match tools::storage::config_path() {
            Ok(path) => match tools::storage::load(&path) {
                Ok(lib) => (lib, Some(path), None),
                Err(err) => (tools::model::ToolLibrary::default(), Some(path), Some(err)),
            },
            Err(err) => (tools::model::ToolLibrary::default(), None, Some(err)),
        };
        let mut app = Self {
            renderer: Arc::new(Mutex::new(mesh_renderer)),
            stock_renderer: Arc::new(Mutex::new(stock_renderer)),
            cam_marker_renderer: Arc::new(Mutex::new(marker_renderer)),
            project: project::Project::default(),
            property_error: String::new(),
            sample_profiles: None,
            section_profiles: None,
            sample_profile_job: None,
            tool_library,
            tool_library_path,
            tool_ui: tools_ui::ToolUi::new(tool_error),
            post_ui: post_ui::PostUi::load(),
            machine_ui: machine_ui::MachineUi::load(),
            nc_export_ui: nc_export_ui::NcExportUi::default(),
            nc_batch_export: cam_combined_nc::NcBatchExportUi::default(),
            nc_playback_ui: nc_playback_ui::NcPlaybackUi::default(),
            test_parallel: test_parallel::TestParallelPanel::default(),
            pending_cam_command: None,
            batch_calculation: None,
            project_path: None,
            recent_projects: recent_projects::RecentProjects::load(),
            current_stl_path: None,
            pending_stl_path: None,
            opening_project: None,
            saved_state: Some(persistence::ProjectState::capture(
                &project::Project::default(),
            )),
            pending_action: None,
            ask_unsaved: false,
            exit_approved: false,
            receiving: None,
            progress: Arc::new(AtomicU32::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
            pending_filename: None,
            pending_success: None,
            filename: "Ningún STL abierto".into(),
            message: "Selecciona un STL binario o abre el ejemplo incluido.".into(),
            yaw: 45f32.to_radians(),
            pitch: 35f32.to_radians(),
            distance: 1.0,
            pan: glam::Vec3::ZERO,
            projection: navigation::Projection::Perspective,
            pending_pick: None,
            pick_result: Arc::new(Mutex::new(None)),
            orbit_pivot: None,
            pivot_press: PivotPress::default(),
            navigation_bindings: NavigationBindings::default(),
            panels: panel_layout::PanelLayout::load(),
            options: options::Options::load(&cc.egui_ctx),
        };
        if let Some(path) = std::env::args_os().nth(1) {
            app.open(PathBuf::from(path), cc.egui_ctx.clone());
        }
        Ok(app)
    }

    fn busy(&self) -> bool {
        self.receiving.is_some() || self.pending_success.is_some()
    }

    fn open(&mut self, path: PathBuf, ctx: egui::Context) {
        self.open_with_context(path, ctx, None);
    }

    fn open_with_context(
        &mut self,
        path: PathBuf,
        ctx: egui::Context,
        opening: Option<OpeningProject>,
    ) {
        if self.busy() {
            return;
        }
        self.sample_profile_job = None;
        self.sample_profiles = None;
        self.pending_pick = None;
        *self.pick_result.lock() = None;
        self.pivot_press = PivotPress::default();
        self.opening_project = opening;
        self.test_parallel.clear_source();
        self.pending_cam_command = None;
        self.batch_calculation = None;
        self.pending_stl_path = Some(path.clone());
        self.renderer.lock().begin_upload();
        self.pending_filename = Some(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );
        self.message = "Leyendo STL y transfiriendo bloques a GPU...".into();
        self.progress.store(0, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
        let (tx, rx) = mpsc::sync_channel(2);
        let progress = Arc::clone(&self.progress);
        let cancel = Arc::clone(&self.cancel);
        self.receiving = Some(rx);
        let worker = std::thread::Builder::new()
            .name("stl-import".into())
            .spawn(move || {
                preview::produce(&path, &progress, &cancel, &tx);
                ctx.request_repaint();
            });
        if let Err(error) = worker {
            self.receiving = None;
            self.pending_filename = None;
            self.pending_stl_path = None;
            self.opening_project = None;
            self.renderer.lock().abort_upload();
            self.message = format!("No se pudo iniciar la importación: {error}");
        }
    }

    #[allow(clippy::collapsible_if)]
    fn poll(&mut self, ctx: &egui::Context) {
        let (error, completed, ready) = {
            let mut renderer = self.renderer.lock();
            (
                renderer.take_error(),
                renderer.take_completed(),
                renderer.can_accept_chunk(),
            )
        };
        if let Some(error) = error {
            self.cancel.store(true, Ordering::Relaxed);
            self.renderer.lock().abort_upload();
            self.pending_success = None;
            self.pending_filename = None;
            self.pending_stl_path = None;
            self.opening_project = None;
            self.message = format!("ERROR GPU: {error}. Se conserva el modelo anterior.");
        }
        if completed {
            if let Some(success) = self.pending_success.take() {
                self.section_profiles = None;
                self.sample_profiles = None; // CSV previo deja de corresponder al modelo cargado.
                self.filename = self.pending_filename.take().unwrap_or_default();
                self.current_stl_path = self.pending_stl_path.take();
                if let Some(opening) = self.opening_project.take() {
                    // Se ha validado el estado y los límites antes del commit GPU.
                    let project = opening
                        .document
                        .state
                        .restore(self.filename.clone(), Some(success.info))
                        .expect("Proyecto verificado antes de finalizar subida GPU");
                    self.project = project;
                    self.saved_state = if opening.relocated {
                        None // Permite guardar la nueva ruta del STL.
                    } else {
                        Some(persistence::ProjectState::capture(&self.project))
                    };
                    self.project_path = Some(opening.path.clone());
                    self.message = match opening.warning {
                        Some(warning) => {
                            format!("Proyecto recuperado: {}. {warning}", self.filename)
                        }
                        None => format!("Proyecto recuperado correctamente: {}", self.filename),
                    };
                    // UI-07: el historial solo cambia al finalizar la carga real en GPU.
                    if let Err(error) = self.recent_projects.record(&opening.path) {
                        self.message
                            .push_str(&format!(" Aviso: recientes no actualizado: {error}"));
                    }
                } else {
                    self.project.loaded(self.filename.clone(), success.info);
                    self.project_path = None;
                    self.saved_state = None;
                    self.message = format!(
                        "STL cargado (CPU {:.1} s). Guarda un proyecto para conservar la configuración.",
                        success.time_seconds
                    );
                }
                self.property_error.clear();
                self.center_view();
            }
        }
        if ready {
            if let Some(rx) = &self.receiving {
                match rx.try_recv() {
                    Ok(LoadEvent::Chunk(chunk)) => {
                        if !self.cancel.load(Ordering::Relaxed) {
                            self.renderer.lock().queue_chunk(chunk);
                            ctx.request_repaint();
                        }
                    }
                    Ok(LoadEvent::Finished { info, time_seconds }) => {
                        self.receiving = None;
                        // Restaurar el proyecto con el STL actual. Sus diferencias solo generan aviso.
                        let validation_error = self.opening_project.as_mut().and_then(|opening| {
                            let check = opening.document.check_loaded(info);
                            if let Ok(Some(warning)) = &check {
                                opening.warning = Some(warning.clone());
                            }
                            check.err().or_else(|| {
                                opening
                                    .document
                                    .state
                                    .restore(
                                        self.pending_filename.clone().unwrap_or_default(),
                                        Some(info),
                                    )
                                    .err()
                            })
                        });
                        let cancelled = self.cancel.load(Ordering::Relaxed);
                        if cancelled || validation_error.is_some() {
                            self.renderer.lock().abort_upload();
                            self.pending_filename = None;
                            self.pending_stl_path = None;
                            self.opening_project = None;
                            self.message = if cancelled {
                                stl::CANCELLED.into()
                            } else {
                                format!(
                                    "ERROR: {}. Se conserva el proyecto anterior.",
                                    validation_error.unwrap_or_default()
                                )
                            };
                        } else {
                            self.pending_success = Some(PendingSuccess { info, time_seconds });
                            self.renderer.lock().finish_upload();
                            self.message = "Finalizando carga en GPU...".into();
                        }
                    }
                    Ok(LoadEvent::Failed(error)) => {
                        self.receiving = None;
                        self.renderer.lock().abort_upload();
                        self.pending_filename = None;
                        self.pending_stl_path = None;
                        self.opening_project = None;
                        if !self.message.starts_with("ERROR GPU:") {
                            self.message = if error == stl::CANCELLED {
                                error
                            } else {
                                format!("ERROR: {error}. Se conserva el modelo anterior.")
                            };
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.receiving = None;
                        self.renderer.lock().abort_upload();
                        self.pending_filename = None;
                        self.pending_stl_path = None;
                        self.opening_project = None;
                        self.message = "ERROR: se interrumpió la lectura".into();
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        // Resultado de la muestra geométrica calculada en segundo plano.
        let profile_result = self.sample_profile_job.as_ref().and_then(|job| job.poll());
        if let Some(result) = profile_result {
            self.sample_profile_job = None;
            match result {
                Ok(outcome) => {
                    self.section_profiles = None;
                    self.sample_profiles = Some(outcome.profiles);
                    self.message = format!(
                        "5 perfiles calculados: 405 puntos, {} contactos. Solo visualización.",
                        outcome.contact_count
                    );
                }
                Err(error) => {
                    self.sample_profiles = None;
                    self.message = format!("ERROR: {error}");
                }
            }
        } else if self.sample_profile_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        // El callback de OpenGL deja el resultado para el siguiente frame egui.
        let hit = { self.pick_result.lock().take() };
        if let Some(hit) = hit {
            match hit {
                Ok(Some(point)) => {
                    self.orbit_pivot = Some(point);
                    self.message = "Pivote seleccionado. Arrastra para girar a su alrededor; Vista > Centrar para restablecer.".into();
                }
                Ok(None) => {
                    self.message =
                        "No hay superficie bajo el clic; pivote anterior conservado.".into();
                }
                Err(error) => {
                    self.message = format!("ERROR: No se pudo establecer el pivote: {error}");
                }
            }
            // Resolver primero el pivote y luego aplicar el movimiento retenido.
            // Si no se encontro superficie, giramos con el pivote anterior.
            let deferred = self.pivot_press.complete();
            if deferred != egui::Vec2::ZERO {
                self.orbit_by(deferred);
            }
            ctx.request_repaint();
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn top_view(&mut self) {
        // Mantener el centro de navegacion y el zoom actuales.
        self.yaw = navigation::TOP_VIEW_YAW;
        self.pitch = navigation::TOP_VIEW_PITCH;
        self.projection = navigation::Projection::Orthographic;
        // Descartar un picking pendiente de la orientacion anterior.
        self.pending_pick = None;
        *self.pick_result.lock() = None;
        self.pivot_press = PivotPress::default();
    }

    fn cube_view(&mut self, target: nav_cube::SnapTarget) {
        // El cubo solo modifica la camara; mantiene el centro y la escala.
        self.yaw = target.yaw;
        self.pitch = target.pitch;
        if target.axial {
            self.projection = navigation::Projection::Orthographic;
        }
        self.pending_pick = None;
        *self.pick_result.lock() = None;
        self.pivot_press = PivotPress::default();
    }
    fn center_view(&mut self) {
        self.yaw = 45f32.to_radians();
        self.pitch = 35f32.to_radians();
        self.distance = 1.0;
        self.pan = glam::Vec3::ZERO;
        self.orbit_pivot = None;
        self.pending_pick = None;
        *self.pick_result.lock() = None;
        self.pivot_press = PivotPress::default();
    }

    fn orbit_by(&mut self, drag: egui::Vec2) {
        let next_yaw = self.yaw - drag.x * 0.008;
        // Admitir +/-90 grados sin saltar a 83 grados en el primer arrastre.
        let next_pitch = (self.pitch + drag.y * 0.008)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        if let Some(pivot) = self.orbit_pivot {
            self.pan = navigation::orbit_target_around_pivot(
                self.pan, pivot, self.yaw, self.pitch, next_yaw, next_pitch,
            );
        }
        self.yaw = next_yaw;
        self.pitch = next_pitch;
    }

    fn file_dialog(&mut self, ctx: egui::Context) {
        self.request_action(FileAction::OpenStl, ctx);
    }
    fn is_dirty(&self) -> bool {
        let current = persistence::ProjectState::capture(&self.project);
        match &self.saved_state {
            Some(saved) => *saved != current,
            None => self.project.model.is_some() || !self.project.operations.items.is_empty(),
        }
    }

    fn request_action(&mut self, action: FileAction, ctx: egui::Context) {
        if self.busy() {
            return;
        }
        if self.is_dirty() {
            self.pending_action = Some(action);
            self.ask_unsaved = true;
        } else {
            self.execute_action(action, ctx);
        }
    }

    fn execute_action(&mut self, action: FileAction, ctx: egui::Context) {
        match action {
            FileAction::New => {
                self.test_parallel.clear_source();
                self.pending_cam_command = None;
                self.batch_calculation = None;
                self.section_profiles = None;
                self.sample_profile_job = None;
                self.renderer.lock().clear_scene();
                self.sample_profiles = None;
                self.project = project::Project::default();
                self.saved_state = Some(persistence::ProjectState::capture(&self.project));
                self.current_stl_path = None;
                self.project_path = None;
                self.filename = "Ningún STL abierto".into();
                self.message = "Nuevo proyecto sin guardar".into();
                self.property_error.clear();
                self.center_view();
            }
            FileAction::OpenProject => self.open_project_dialog(ctx),
            FileAction::OpenRecent(path) => self.open_project_path(path, ctx),
            FileAction::OpenStl => self.file_dialog_raw(ctx),
            FileAction::Exit => {
                self.exit_approved = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn save_project(&mut self, save_as: bool) -> bool {
        if self.busy() {
            self.message = "ERROR: espera a que finalice la importación STL".into();
            return false;
        }
        let target = if !save_as {
            self.project_path.clone()
        } else {
            None
        };
        let target = match target {
            Some(path) => path,
            None => {
                let mut dialog =
                    rfd::FileDialog::new().add_filter("Proyecto LaguCAM", &["lagucam"]);
                if let Some(stl) = &self.current_stl_path
                    && let Some(stem) = stl.file_stem().and_then(|v| v.to_str())
                {
                    dialog = dialog.set_file_name(format!("{stem}.lagucam"));
                }
                let Some(path) = dialog.save_file() else {
                    return false;
                };
                if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lagucam"))
                {
                    path
                } else {
                    path.with_extension("lagucam")
                }
            }
        };
        let result = (|| -> Result<(), String> {
            let doc = persistence::ProjectDocument::from_project(
                &self.project,
                self.current_stl_path.as_deref(),
                &target,
            )?;
            persistence::write_project(&target, &doc)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.project_path = Some(target.clone());
                self.saved_state = Some(persistence::ProjectState::capture(&self.project));
                self.message = format!("Proyecto guardado: {}", target.display());
                if let Err(error) = self.recent_projects.record(&target) {
                    self.message
                        .push_str(&format!(" Aviso: recientes no actualizado: {error}"));
                }
                true
            }
            Err(error) => {
                self.message = format!("ERROR al guardar proyecto: {error}");
                false
            }
        }
    }

    fn open_project_dialog(&mut self, ctx: egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Proyecto LaguCAM", &["lagucam"])
            .pick_file()
        else {
            return;
        };
        self.open_project_path(path, ctx);
    }

    fn open_project_path(&mut self, path: PathBuf, ctx: egui::Context) {
        let result = (|| -> Result<(), String> {
            let doc = persistence::read_project(&path)?;
            let Some(mut stl_path) = doc.resolve_stl(&path) else {
                let restored = doc.state.restore(String::new(), None)?;
                self.test_parallel.clear_source();
                self.pending_cam_command = None;
                self.batch_calculation = None;
                self.section_profiles = None;
                self.renderer.lock().clear_scene();
                self.sample_profile_job = None;
                self.sample_profiles = None;
                self.project = restored;
                self.project_path = Some(path.clone());
                self.current_stl_path = None;
                self.saved_state = Some(persistence::ProjectState::capture(&self.project));
                self.filename = "Ningún STL abierto".into();
                self.property_error.clear();
                self.message = format!("Proyecto abierto: {}", path.display());
                if let Err(error) = self.recent_projects.record(&path) {
                    self.message
                        .push_str(&format!(" Aviso: recientes no actualizado: {error}"));
                }
                return Ok(());
            };
            // Si la referencia se perdió o difiere, no importar un STL equivocado.
            // Ofrecemos localizarlo y volvemos a verificar las muestras.
            let warning = if doc.check_stl(&stl_path).is_err() {
                let Some(located) = rfd::FileDialog::new()
                    .set_title("STL no encontrado o diferente: selecciona el original")
                    .add_filter("STL binario", &["stl"])
                    .pick_file()
                else {
                    return Err("No se seleccionó el STL original".into());
                };
                let warning = doc.check_stl(&located)?;
                stl_path = located;
                warning
            } else {
                doc.check_stl(&stl_path)?
            };
            let relocated = doc.resolve_stl(&path).as_deref() != Some(stl_path.as_path());
            // El estado anterior permanece activo hasta confirmar carga y metadatos.
            self.open_with_context(
                stl_path,
                ctx,
                Some(OpeningProject {
                    document: doc,
                    path,
                    relocated,
                    warning,
                }),
            );
            Ok(())
        })();
        if let Err(error) = result {
            self.message = format!("ERROR al abrir proyecto: {error}");
        }
    }

    fn file_dialog_raw(&mut self, ctx: egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("STL binario", &["stl"])
            .pick_file()
        {
            self.open(path, ctx);
        }
    }

    fn unsaved_dialog(&mut self, ctx: &egui::Context) {
        if !self.ask_unsaved {
            return;
        }
        let mut choice: Option<u8> = None;
        egui::Window::new("Cambios sin guardar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Este proyecto contiene modificaciones sin guardar.");
                ui.label("¿Quieres guardarlas antes de continuar?");
                ui.horizontal(|ui| {
                    if ui.button("Guardar").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Descartar").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancelar").clicked() {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => {
                if self.save_project(false) {
                    self.ask_unsaved = false;
                    if let Some(action) = self.pending_action.take() {
                        self.execute_action(action, ctx.clone());
                    }
                }
            }
            Some(1) => {
                self.ask_unsaved = false;
                if let Some(action) = self.pending_action.take() {
                    self.execute_action(action, ctx.clone());
                }
            }
            Some(2) => {
                self.ask_unsaved = false;
                self.pending_action = None;
            }
            _ => {}
        }
    }
    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Archivo", |ui| {
                let enabled = !self.busy();
                if ui
                    .add_enabled(enabled, egui::Button::new("Nuevo proyecto"))
                    .clicked()
                {
                    ui.close();
                    self.request_action(FileAction::New, ui.ctx().clone());
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Abrir proyecto…"))
                    .clicked()
                {
                    ui.close();
                    self.request_action(FileAction::OpenProject, ui.ctx().clone());
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Guardar proyecto"))
                    .clicked()
                {
                    ui.close();
                    self.save_project(false);
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Guardar proyecto como…"))
                    .clicked()
                {
                    ui.close();
                    self.save_project(true);
                }
                if !self.recent_projects.entries().is_empty() {
                    ui.separator();
                    // UI-07b: papelera vectorial junto al encabezado de recientes.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("PROYECTOS RECIENTES").small().weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let clear = ui
                                .add_enabled(
                                    enabled,
                                    egui::Button::new("")
                                        .min_size(egui::vec2(23.0, 21.0))
                                        .frame(false),
                                )
                                .on_hover_text("Limpiar historial de proyectos recientes");

                            let center = clear.rect.center();
                            let color = if !enabled {
                                ui.visuals().widgets.noninteractive.fg_stroke.color
                            } else if clear.hovered() {
                                ui.visuals().widgets.hovered.fg_stroke.color
                            } else {
                                ui.visuals().widgets.inactive.fg_stroke.color
                            };
                            let stroke = egui::Stroke::new(1.4, color);
                            let point = |x: f32, y: f32| center + egui::vec2(x, y);
                            let painter = ui.painter();
                            painter.line_segment([point(-5.0, -4.5), point(5.0, -4.5)], stroke);
                            painter.line_segment([point(-2.0, -6.0), point(2.0, -6.0)], stroke);
                            painter.line_segment([point(-4.0, -2.7), point(-3.0, 5.5)], stroke);
                            painter.line_segment([point(4.0, -2.7), point(3.0, 5.5)], stroke);
                            painter.line_segment([point(-3.0, 5.5), point(3.0, 5.5)], stroke);
                            painter.line_segment([point(-1.2, -2.0), point(-1.2, 3.7)], stroke);
                            painter.line_segment([point(1.2, -2.0), point(1.2, 3.7)], stroke);

                            if clear.clicked() {
                                ui.close();
                                if let Err(error) = self.recent_projects.clear() {
                                    self.message = format!("ERROR al limpiar recientes: {error}");
                                }
                            }
                        });
                    });
                    // Copiar rutas evita retener una referencia al estado al lanzar la acción.
                    for path in self.recent_projects.entries().to_vec() {
                        let exists = path.is_file();
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        let label = if exists { name } else { format!("{name} (no encontrado)") };
                        if ui
                            .add_enabled(enabled && exists, egui::Button::new(label))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            ui.close();
                            self.request_action(FileAction::OpenRecent(path), ui.ctx().clone());
                        }
                    }
                }
                if let Some(error) = self.recent_projects.error() {
                    ui.label(egui::RichText::new("Aviso: historial no disponible").small().weak())
                        .on_hover_text(error);
                }
                ui.separator();
                if ui
                    .add_enabled(enabled, egui::Button::new("Abrir STL…"))
                    .clicked()
                {
                    ui.close();
                    self.file_dialog(ui.ctx().clone());
                }
                ui.separator();

                if ui.button("Salir").clicked() {
                    ui.close();
                    self.request_action(FileAction::Exit, ui.ctx().clone());
                }
            });
            ui.menu_button("Herramientas", |ui| {
                if ui.button("Biblioteca de herramientas...").clicked() {
                    self.tool_ui.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Configuracion de maquina...").clicked() {
                    self.machine_ui.open = true;
                    ui.close();
                }
                if ui.button("Configuracion de postprocesador...").clicked() {
                    self.post_ui.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Opciones…").clicked() {
                    self.options.open = true;
                    ui.close();
                }
            });
            ui.menu_button("Vista", |ui| {
                ui.menu_button("Diagnóstico geométrico (experimental)", |ui| {
                if ui
                    .add_enabled(
                        !self.busy() && self.project.model.is_some() && self.sample_profile_job.is_none(),
                        egui::Button::new("Mostrar secciones reales CSV..."),
                    )
                    .clicked()
                {
                    ui.close();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Secciones reales LaguCAM", &["csv"])
                        .pick_file()
                        && let Some(model) = self.project.model.as_ref()
                    {
                        match section_overlay::Sections::load(&path, model) {
                                Ok(sections) => {
                                    let count = sections.piece_count();
                                    self.sample_profile_job = None;
                                    self.sample_profiles = None;
                                    self.section_profiles = Some(sections);
                                    self.message = format!(
                                        "Secciones reales cargadas: {count} elementos. Solo visualizacion."
                                    );
                                }
                                Err(error) => self.message = format!("ERROR: {error}"),
                            }
                    }
                }
                if ui.add_enabled(self.section_profiles.is_some(),
                    egui::Button::new("Ocultar secciones reales")).clicked()
                {
                    self.section_profiles = None;
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.busy()
                            && self.project.model.is_some()
                            && self.current_stl_path.is_some()
                            && self.sample_profile_job.is_none(),
                        egui::Button::new("Calcular 5 perfiles del STL abierto"),
                    )
                    .clicked()
                {
                    ui.close();
                    if let (Some(path), Some(model)) =
                        (self.current_stl_path.clone(), self.project.model.as_ref())
                    {
                        match sample_direct::ProfileJob::start(
                            path,
                            model.bounds,
                            model.size_bytes,
                            model.triangles,
                            ui.ctx().clone(),
                        ) {
                            Ok(job) => {
                                self.sample_profiles = None;
                                self.sample_profile_job = Some(job);
                                self.message =
                                    "Calculando 5 perfiles sobre el STL abierto en segundo plano..."
                                        .into();
                            }
                            Err(error) => self.message = format!("ERROR: {error}"),
                        }
                    }
                }
                if ui
                    .add_enabled(
                        !self.busy() && self.project.model.is_some(),
                        egui::Button::new("Mostrar perfiles de muestra CSV…"),
                    )
                    .clicked()
                {
                    ui.close();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Muestra de perfiles LaguCAM", &["csv"])
                        .pick_file()
                    {
                        match sample_overlay::Profiles::load(&path) {
                            Ok(profiles) => {
                                self.sample_profile_job = None;
                                self.section_profiles = None;
                                self.sample_profiles = Some(profiles);
                                self.message =
                                    "Perfiles cargados: vista de muestra, no trayectoria CNC"
                                        .into();
                            }
                            Err(err) => self.message = format!("ERROR: {err}"),
                        }
                    }
                }
                if ui
                    .add_enabled(
                        self.sample_profiles.is_some() || self.sample_profile_job.is_some(),
                        egui::Button::new("Ocultar perfiles de muestra"),
                    )
                    .clicked()
                {
                    self.sample_profile_job = None;
                    self.sample_profiles = None;
                    ui.close();
                }
                ui.separator();
                });
                ui.separator();
                if ui.button("Vista superior (+Z)").clicked() {
                    self.top_view();
                    ui.ctx().request_repaint();
                    ui.close();
                }
                if ui.button("Centrar vista").clicked() {
                    self.center_view();
                    ui.ctx().request_repaint();
                    ui.close();
                }
                if ui.button("Restablecer paneles").clicked() {
                    self.panels.reset();
                    ui.close();
                }
            });
        });
    }
    /// Una sola operacion; el conjunto se integrara sin simular concatenaciones.
    fn execute_ready_cam_command(
        &mut self,
        command: project_ui::OperationCommand,
    ) -> Result<(), String> {
        use project_ui::OperationCommand;
        let Some(index) = command.index() else {
            return Err("La accion conjunta no corresponde a una operacion individual".into());
        };
        let op = self
            .project
            .operations
            .items
            .get(index)
            .ok_or("La operacion solicitada ya no existe")?;
        if op.status != project::operations::OperationStatus::Ready
            || self.test_parallel.nc_result(index).is_none()
        {
            return Err("La operacion no dispone de una trayectoria CAM valida".into());
        }
        match command {
            OperationCommand::Calculate(_) => Ok(()),
            OperationCommand::CalculateAll => {
                Err("El calculo conjunto requiere su propia cola".into())
            }
            OperationCommand::GenerateAllNc => {
                Err("La generacion conjunta no corresponde a una operacion individual".into())
            }
            OperationCommand::GenerateNc(_) => {
                self.nc_export_ui.open_for_operation(index);
                Ok(())
            }
            OperationCommand::Simulate(_) => {
                if self.nc_playback_ui.simulation_open() {
                    return Err("Cierra la simulacion anterior antes de iniciar otra".into());
                }
                let (text, xyz, tool) = self.nc_export_ui.playback_for_operation(
                    index,
                    &self.project,
                    &self.test_parallel,
                    &self.post_ui,
                )?;
                self.nc_playback_ui.load_generated_compact(
                    text,
                    xyz,
                    tool,
                    &self.project,
                    &self.tool_library,
                )?;
                Ok(())
            }
        }
    }

    fn request_cam_command(&mut self, command: project_ui::OperationCommand, ctx: &egui::Context) {
        use project_ui::OperationCommand;
        if command == OperationCommand::GenerateAllNc {
            if self.project.selection != project::selection::Selection::Operations {
                self.message = "ERROR: Selecciona el nodo Operaciones".into();
            } else if self.batch_calculation.is_some() || self.test_parallel.calculation_running() {
                self.message = "ERROR: Espera a que termine el calculo CAM".into();
            } else {
                self.nc_batch_export.open_for_batch();
                self.message =
                    "Generando NC conjunto: comprueba los resultados en su ventana".into();
            }
            return;
        }
        if command == OperationCommand::CalculateAll {
            if self.project.selection != project::selection::Selection::Operations {
                self.message = "ERROR: Selecciona el nodo Operaciones".into();
            } else {
                self.start_batch_calculation(ctx);
            }
            return;
        }
        if self.batch_calculation.is_some() {
            self.message =
                "ERROR: Termina o cancela el calculo conjunto antes de iniciar otro".into();
            return;
        }
        let Some(index) = command.index() else {
            return;
        };
        if !matches!(self.project.selection, project::selection::Selection::Operation(i) if i == index)
        {
            self.message = "ERROR: La seleccion de la operacion ha cambiado".into();
            return;
        }
        if matches!(command, OperationCommand::Simulate(_)) && self.nc_playback_ui.simulation_open()
        {
            self.message = "ERROR: Cierra la simulacion anterior".into();
            return;
        }
        if self.test_parallel.nc_result(index).is_some()
            && self
                .project
                .operations
                .items
                .get(index)
                .is_some_and(|op| op.status == project::operations::OperationStatus::Ready)
        {
            if let Err(error) = self.execute_ready_cam_command(command) {
                self.message = format!("ERROR: {error}");
            } else if matches!(command, OperationCommand::Calculate(_)) {
                self.message = "Trayectoria calculada y vigente; no es necesario recalcular".into();
            }
            return;
        }
        if self.test_parallel.is_calculating(index) {
            if !matches!(command, OperationCommand::Calculate(_)) {
                self.pending_cam_command = Some(command);
                self.message = "Calculo en curso; la accion continuara cuando termine".into();
            }
            return;
        }
        if self.test_parallel.calculation_running() {
            self.message = "ERROR: Ya se esta calculando otra operacion".into();
            return;
        }
        match self.test_parallel.calculate_operation(
            index,
            &mut self.project,
            self.current_stl_path.as_deref(),
            &self.tool_library,
            ctx,
        ) {
            Ok(()) => {
                self.pending_cam_command = if matches!(command, OperationCommand::Calculate(_)) {
                    None
                } else {
                    Some(command)
                };
                self.message = "Calculo CAM iniciado".into();
            }
            Err(error) => self.message = format!("ERROR: {error}"),
        }
    }

    fn resume_pending_cam_command(&mut self) {
        let Some(command) = self.pending_cam_command else {
            return;
        };
        if self.test_parallel.calculation_running() {
            return;
        }
        self.pending_cam_command = None;
        let Some(index) = command.index() else {
            return;
        };
        if self.project.operations.items.get(index).is_none() {
            self.message = "ERROR: La operacion desaparecio durante el calculo".into();
            return;
        }
        if let Err(error) = self.execute_ready_cam_command(command) {
            self.message = format!("ERROR: No se pudo continuar tras el calculo: {error}");
        }
    }

    fn start_batch_calculation(&mut self, ctx: &egui::Context) {
        if self.busy()
            || self.batch_calculation.is_some()
            || self.test_parallel.calculation_running()
            || self.pending_cam_command.is_some()
        {
            self.message = "ERROR: Ya existe otra tarea CAM en curso".into();
            return;
        }
        if self.current_stl_path.is_none() || self.project.model.is_none() {
            self.message = "ERROR: Abre primero un modelo STL para calcular las operaciones".into();
            return;
        }
        let Some(batch) = cam_batch::BatchCalculation::from_project(&self.project) else {
            self.message = "Calculo conjunto: no hay operaciones activas".into();
            return;
        };
        self.batch_calculation = Some(batch);
        self.continue_batch_calculation(ctx);
    }

    /// El solver solo acepta un trabajo: avanzar cuando el anterior termino.
    fn continue_batch_calculation(&mut self, ctx: &egui::Context) {
        let Some(mut batch) = self.batch_calculation.take() else {
            return;
        };
        if self.test_parallel.calculation_running() {
            self.batch_calculation = Some(batch);
            return;
        }
        if let Some(index) = batch.current.take() {
            if self
                .project
                .operations
                .items
                .get(index)
                .is_some_and(|op| op.status == project::operations::OperationStatus::Ready)
                && self.test_parallel.nc_result(index).is_some()
            {
                batch.calculated += 1;
            } else {
                let name = self
                    .project
                    .operations
                    .items
                    .get(index)
                    .map_or_else(|| format!("Operacion {}", index + 1), |op| op.name.clone());
                batch.errors.push(format!(
                    "{name}: {}",
                    self.test_parallel.calculation_message()
                ));
            }
        }
        while let Some(index) = batch.queued.pop_front() {
            let Some(operation) = self.project.operations.items.get(index) else {
                batch
                    .errors
                    .push(format!("La operacion {} desaparecio", index + 1));
                continue;
            };
            let name = operation.name.clone();
            if !operation.active {
                batch.skipped += 1;
                continue;
            }
            if operation.status == project::operations::OperationStatus::Ready
                && self.test_parallel.nc_result(index).is_some()
            {
                batch.reused += 1;
                continue;
            }
            match self.test_parallel.calculate_operation(
                index,
                &mut self.project,
                self.current_stl_path.as_deref(),
                &self.tool_library,
                ctx,
            ) {
                Ok(()) => {
                    batch.current = Some(index);
                    self.message = format!(
                        "Calculo conjunto: {} / {} completadas. Calculando {name}",
                        batch.completed(),
                        batch.total
                    );
                    self.batch_calculation = Some(batch);
                    ctx.request_repaint();
                    return;
                }
                Err(error) => batch.errors.push(format!("{name}: {error}")),
            }
        }
        let ready = batch.calculated + batch.reused;
        if let Some(first) = batch.errors.first() {
            self.message = format!(
                "ERROR: Calculo conjunto: {ready}/{} listas ({} nuevas, {} reutilizadas); {} errores, {} desactivadas. Primera: {first}",
                batch.total,
                batch.calculated,
                batch.reused,
                batch.errors.len(),
                batch.skipped
            );
        } else {
            self.message = format!(
                "Calculo conjunto terminado: {ready}/{} listas ({} nuevas, {} reutilizadas, {} desactivadas)",
                batch.total, batch.calculated, batch.reused, batch.skipped
            );
        }
        ctx.request_repaint();
    }
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if let Some(path) = &self.project_path {
                ui.small(format!("Proyecto: {}", path.display()));
            }
            if self.is_dirty() {
                ui.colored_label(egui::Color32::YELLOW, "Cambios sin guardar");
            }
            if self.busy() {
                if let Some(name) = &self.pending_filename {
                    ui.label(format!("Importando: {name}"));
                }
                let fraction = self.progress.load(Ordering::Relaxed) as f32 / 100.0;
                ui.add(egui::ProgressBar::new(fraction.min(0.99)).show_percentage());
                ui.small(&self.message);
                if ui.button("Cancelar importación").clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.renderer.lock().abort_upload();
                    if self.receiving.is_none() {
                        self.pending_success = None;
                        self.pending_filename = None;
                        self.message = stl::CANCELLED.into();
                    } else {
                        self.message = "Cancelando importación...".into();
                    }
                }
            } else if let Some(batch) = &self.batch_calculation {
                ui.label(format!(
                    "Calculo conjunto: {} / {} operaciones",
                    batch.completed(),
                    batch.total
                ));
                if ui.button("Cancelar conjunto").clicked() {
                    self.test_parallel.cancel_current_calculation();
                    self.batch_calculation = None;
                    self.message = "Calculo conjunto cancelado".into();
                }
            } else if self.message.starts_with("ERROR") {
                ui.colored_label(egui::Color32::RED, &self.message);
            } else if self.message.starts_with("Calculo conjunto") {
                ui.small(&self.message);
            }
        });
    }
    fn viewport(&mut self, ui: &mut egui::Ui) {
        // El indicador sigue la accion de orbita, no un boton fisico fijo.
        // Si se suelta incluso fuera de la vista, desaparece en el siguiente frame.
        let bindings = self.navigation_bindings;
        self.pivot_press.sync_held(bindings.orbit_down(ui.ctx()));
        let size = ui.available_size();
        if size.x <= 5.0 || size.y <= 5.0 {
            return;
        }
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        // Cubo visible en el visor; se oculta en superficies demasiado estrechas.
        // El selector de proyeccion permanece accesible bajo el cubo.
        let cube_rect = nav_cube::viewport_rect(rect);
        // V3D-06: proyeccion como dos modos excluyentes bajo el cubo.
        // El ancho y centro coinciden con el cubo; en visores pequenos se oculta.
        let projection_top = cube_rect.map_or(rect.top() + 10.0, |area| area.bottom() + 8.0);
        let projection_control_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 126.0, projection_top),
            egui::vec2(114.0, 26.0),
        );
        let show_projection_control =
            rect.width() >= 180.0 && projection_control_rect.bottom() <= rect.bottom() - 8.0;
        let over_projection_control = show_projection_control
            && response
                .hover_pos()
                .is_some_and(|pos| projection_control_rect.contains(pos));
        let over_nav_cube = cube_rect
            .is_some_and(|area| response.hover_pos().is_some_and(|pos| area.contains(pos)));
        // Dibujado antes del callback OpenGL; no se modifica la malla STL.
        // Cuatro vértices: azul más intenso arriba y claro abajo.
        let mut sky = egui::Mesh::default();
        let high = egui::Color32::from_rgb(75, 153, 210);
        let low = egui::Color32::from_rgb(190, 228, 246);
        sky.colored_vertex(rect.left_top(), high);
        sky.colored_vertex(rect.right_top(), high);
        sky.colored_vertex(rect.right_bottom(), low);
        sky.colored_vertex(rect.left_bottom(), low);
        sky.add_triangle(0, 1, 2);
        sky.add_triangle(0, 2, 3);
        ui.painter().add(egui::Shape::mesh(sky));
        // No esperar a clicked(): ese evento llega al SOLTAR y no existe
        // si el mismo gesto se transforma en arrastre.
        // V3D-05: el pivote sigue el puntero tambien al editar Origen CNC.
        // El clic sin arrastre en un marcador rojo sigue gestionado por overlay::paint.
        if bindings.orbit_pressed(ui.ctx())
            && response.hovered()
            && !over_projection_control
            && !over_nav_cube
            && !self.busy()
            && self.project.model.is_some()
            && let Some(pos) = response
                .interact_pointer_pos()
                .filter(|p| rect.contains(*p))
        {
            self.pending_pick = Some(pos);
            self.pivot_press.start();
            ui.ctx().request_repaint();
        }
        if !over_nav_cube && bindings.orbit_dragged(&response) {
            // La seleccion mediante GL termina antes de ejecutar el arrastre.
            // La primera fraccion de movimiento no se pierde ni usa el pivote viejo.
            if let Some(drag) = self.pivot_press.motion(response.drag_motion()) {
                self.orbit_by(drag);
            }
            ui.ctx().request_repaint();
        }
        if !over_nav_cube && bindings.pan_dragged(&response) {
            let drag = response.drag_motion();
            // Usar la misma base que la proyeccion y el zoom:
            // Z x direccion se anula en una vista superior exacta.
            let (right, up, _) = navigation::camera_axes(self.yaw, self.pitch);
            let distance = render::fit_distance(rect.width() / rect.height()) * self.distance;
            let units = 2.0 * distance * (22.5f32.to_radians()).tan() / rect.height();
            self.pan += (-right * drag.x + up * drag.y) * units;
            ui.ctx().request_repaint();
        }
        if response.hovered() && !over_projection_control && !over_nav_cube {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll.is_finite() && scroll != 0.0 {
                let old_zoom = self.distance;
                let factor = (-scroll * 0.003).clamp(-2.0, 2.0).exp();
                let new_zoom = (old_zoom * factor).clamp(0.05, 20.0);
                if new_zoom != old_zoom
                    && let Some(pointer) = response.hover_pos()
                {
                    // Las posiciones del raton y del viewport son puntos logicos de egui.
                    // Su cociente produce NDC, independiente de DPI y del STL.
                    let cursor = glam::Vec2::new(
                        2.0 * (pointer.x - rect.left()) / rect.width() - 1.0,
                        1.0 - 2.0 * (pointer.y - rect.top()) / rect.height(),
                    );
                    self.pan = navigation::zoom_target_at_cursor(
                        self.pan,
                        self.yaw,
                        self.pitch,
                        old_zoom,
                        new_zoom,
                        rect.width() / rect.height(),
                        cursor,
                    );
                    self.distance = new_zoom;
                    ui.ctx().request_repaint();
                }
            }
        }
        let yaw = self.yaw;
        let pitch = self.pitch;
        let distance = self.distance;
        let pan = self.pan;
        let projection = self.projection;
        let aspect = rect.width() / rect.height().max(1.0);
        let model_color = self.project.model_color;
        let visual_transform = self
            .project
            .model
            .as_ref()
            .and_then(|model| {
                self.project
                    .model_transform
                    .preview_matrix(&model.raw_bounds, &model.bounds)
                    .ok()
            })
            .unwrap_or(glam::Mat4::IDENTITY);
        let renderer = Arc::clone(&self.renderer);
        let stock_renderer = Arc::clone(&self.stock_renderer);
        let cam_marker_renderer = Arc::clone(&self.cam_marker_renderer);
        let stock_geometry = stock_render::StockGeometry::from_project(&self.project);
        let marker_instances = self.project.model.as_ref().map_or_else(Vec::new, |model| {
            self.test_parallel.marker_instances_3d(model)
        });
        let picking = self.pending_pick.take();
        let pick_result = Arc::clone(&self.pick_result);
        let pick_ctx = ui.ctx().clone();
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                use glow::HasContext as _;
                let clip = egui::PaintCallbackInfo {
                    clip_rect: info.clip_rect.intersect(info.viewport),
                    ..info
                }
                .clip_rect_in_pixels();
                unsafe {
                    painter.gl().scissor(
                        clip.left_px,
                        clip.from_bottom_px,
                        clip.width_px,
                        clip.height_px,
                    );
                }
                {
                    let mut mesh = renderer.lock();
                    mesh.set_model_color(model_color);
                    mesh.set_visual_transform(visual_transform);
                    mesh.paint(painter.gl(), yaw, pitch, distance, aspect, pan, projection);
                }
                // Consultar la escena renderizada: no se relee el fichero ni se
                // inspeccionan triángulos desde la UI. Solo una vez por clic sobre el modelo.
                if let Some(pos) = picking {
                    let viewport = info.viewport_in_pixels();
                    let x = (pos.x * info.pixels_per_point).floor() as i32;
                    let y = info.screen_size_px[1] as i32
                        - 1
                        - (pos.y * info.pixels_per_point).floor() as i32;
                    let hit = scene_pick::pick_visible_surface(
                        painter.gl(),
                        painter.intermediate_fbo(),
                        scene_pick::PixelViewport {
                            left: viewport.left_px,
                            bottom: viewport.from_bottom_px,
                            width: viewport.width_px,
                            height: viewport.height_px,
                        },
                        x,
                        y,
                        navigation::view_projection_mode(
                            yaw, pitch, distance, aspect, pan, projection,
                        ),
                    );
                    *pick_result.lock() = Some(hit);
                    pick_ctx.request_repaint();
                }
                // El STL ya ha escrito el depth buffer. No se modifica su importador.
                stock_renderer.lock().paint(
                    painter.gl(),
                    stock_geometry.as_ref(),
                    stock_render::Camera {
                        yaw,
                        pitch,
                        zoom: distance,
                        aspect,
                        pan,
                        projection,
                    },
                );
            })),
        });
        overlay::paint(
            ui,
            rect,
            &mut self.project,
            overlay::Camera {
                yaw,
                pitch,
                zoom: distance,
                pan,
                projection,
            },
            response.clicked() && !over_nav_cube,
            response.hover_pos(),
        );
        if self.project.model_transform.is_identity()
            && let (Some(profiles), Some(model)) = (&self.sample_profiles, &self.project.model)
        {
            profiles.paint(ui, rect, model, yaw, pitch, distance, pan, projection);
        }
        if self.project.model_transform.is_identity()
            && let (Some(sections), Some(model)) = (&self.section_profiles, &self.project.model)
        {
            sections.paint(ui, rect, model, yaw, pitch, distance, pan, projection);
        }
        if let Some(model) = &self.project.model {
            self.test_parallel
                .paint(ui, rect, model, yaw, pitch, distance, pan, projection);
        }
        if self.nc_playback_ui.compact_active()
            && self.nc_playback_ui.compact_controls(
                ui.ctx(),
                rect,
                &self.project,
                &self.tool_library,
            )
        {
            self.nc_playback_ui.close_compact();
        }
        self.nc_playback_ui.paint(
            ui,
            rect,
            &self.project,
            yaw,
            pitch,
            distance,
            pan,
            projection,
        );
        self.nc_export_ui.paint_issue_focus(
            ui,
            rect,
            &self.project,
            nc_tool_overlay::Camera {
                yaw,
                pitch,
                distance,
                pan,
                projection,
            },
        );
        // Marcadores renderizados tras las lineas egui: sus fragmentos opacos
        // sustituyen visualmente las lineas que quedan por debajo del solido.
        // No se limpia profundidad: se conserva la del STL en este mismo viewport.
        if !marker_instances.is_empty() {
            ui.painter().add(egui::PaintCallback {
                rect,
                callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                    use glow::HasContext as _;
                    let clip = egui::PaintCallbackInfo {
                        clip_rect: info.clip_rect.intersect(info.viewport),
                        ..info
                    }
                    .clip_rect_in_pixels();
                    unsafe {
                        painter.gl().enable(glow::SCISSOR_TEST);
                        painter.gl().scissor(
                            clip.left_px,
                            clip.from_bottom_px,
                            clip.width_px,
                            clip.height_px,
                        );
                    }
                    cam_marker_renderer.lock().paint(
                        painter.gl(),
                        &marker_instances,
                        stock_render::Camera {
                            yaw,
                            pitch,
                            zoom: distance,
                            aspect,
                            pan,
                            projection,
                        },
                    );
                })),
            });
        }
        if self.pivot_press.marker_visible()
            && let Some(pivot) = self.orbit_pivot
        {
            // El indicador marca el punto REAL seleccionado, no el centro de pantalla.
            let mvp = navigation::view_projection_mode(
                self.yaw,
                self.pitch,
                self.distance,
                rect.width() / rect.height().max(1.0),
                self.pan,
                self.projection,
            );
            let clip = mvp * pivot.extend(1.0);
            if clip.is_finite() && clip.w > 0.0 {
                let ndc = clip.truncate() / clip.w;
                if ndc.is_finite()
                    && ndc.x.abs() <= 1.0
                    && ndc.y.abs() <= 1.0
                    && (-1.0..=1.0).contains(&ndc.z)
                {
                    let p = rect.center()
                        + egui::vec2(ndc.x * rect.width() * 0.5, -ndc.y * rect.height() * 0.5);
                    // Dos trazos contrastados: visible sobre cielo y malla oscura.
                    let outside = egui::Stroke::new(3.0, egui::Color32::BLACK);
                    let inside = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 230, 70));
                    for stroke in [outside, inside] {
                        ui.painter().circle_stroke(p, 6.0, stroke);
                        ui.painter().line_segment(
                            [p + egui::vec2(-10.0, 0.0), p + egui::vec2(10.0, 0.0)],
                            stroke,
                        );
                        ui.painter().line_segment(
                            [p + egui::vec2(0.0, -10.0), p + egui::vec2(0.0, 10.0)],
                            stroke,
                        );
                    }
                }
            }
        }
        ui.painter().text(
            rect.left_top() + egui::vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            "VISTA 3D  ·  Pulsar + arrastrar: pivote y rotación  ·  Zoom: rueda  ·  Desplazar: botón derecho",
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_GRAY,
        );
        if let Some(area) = cube_rect
            && let Some(target) = nav_cube::paint(
                ui,
                area,
                yaw,
                pitch,
                response.hover_pos(),
                response.clicked(),
            )
        {
            self.cube_view(target);
            ui.ctx().request_repaint();
        } // Dibujar despues de la escena para que el control quede delante.
        if show_projection_control {
            // Ambos estados permanecen visibles; solo el activo se resalta.
            // Se toman los colores del tema de egui (claro u oscuro).
            let selected_background = ui.visuals().selection.bg_fill;
            let normal_background = ui.visuals().widgets.inactive.bg_fill;
            let selected_foreground = ui.visuals().selection.stroke.color;
            let normal_foreground = ui.visuals().text_color();
            let border = ui.visuals().widgets.inactive.bg_stroke.color;
            let gap = 4.0;
            let segment_width = (projection_control_rect.width() - gap) / 2.0;
            for (index, (mode, label, help)) in [
                (
                    navigation::Projection::Perspective,
                    "Persp",
                    "Proyección en perspectiva. Conserva orientación y zoom.",
                ),
                (
                    navigation::Projection::Orthographic,
                    "Orto",
                    "Proyección ortográfica. Conserva orientación y zoom.",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let active = self.projection == mode;
                let segment_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        projection_control_rect.left() + index as f32 * (segment_width + gap),
                        projection_control_rect.top(),
                    ),
                    egui::vec2(segment_width, projection_control_rect.height()),
                );
                let text_color = if active {
                    selected_foreground
                } else {
                    normal_foreground
                };
                let fill = if active {
                    selected_background
                } else {
                    normal_background
                };
                let button = egui::Button::new(egui::RichText::new(label).color(text_color))
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, border));
                if ui.place(segment_rect, button).on_hover_text(help).clicked() && !active {
                    self.projection = mode;
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());
        self.test_parallel
            .sync_tools(&mut self.project, &self.tool_library);
        self.test_parallel.poll(ui.ctx(), &mut self.project);
        self.resume_pending_cam_command();
        self.continue_batch_calculation(ui.ctx());
        self.nc_playback_ui.sync_session(
            &self.project,
            &self.tool_library,
            self.current_stl_path.as_deref(),
        );
        egui::Panel::top("main_menu").show(ui, |ui| self.menu(ui));
        if self.busy()
            || self.sample_profile_job.is_some()
            || self.is_dirty()
            || self.project_path.is_some()
            || self.message.starts_with("ERROR")
            || self.message.starts_with("Calculo conjunto")
            || self.batch_calculation.is_some()
        {
            egui::Panel::top("lagucam_status").show(ui, |ui| self.status_bar(ui));
        }
        let enabled = !self.busy();
        let before_transform = self.project.model_transform;
        let simulation_open = self.nc_playback_ui.simulation_open();
        let open_docked = self.panels.show_docked(
            ui,
            &mut self.project,
            &mut self.property_error,
            enabled,
            simulation_open,
        );
        egui::CentralPanel::default().show(ui, |ui| self.viewport(ui));
        let open_floating = self.panels.show_floating(
            ui.ctx(),
            &mut self.project,
            &mut self.property_error,
            enabled,
            simulation_open,
        );
        // El arbol define el orden de mecanizado. El cambio es solo de indice:
        // conservar las trayectorias calculadas, pero no los indices del exportador.
        if let Some((from, to)) = self.panels.take_move_operation()
            && from != to
            && from < self.project.operations.items.len()
            && to < self.project.operations.items.len()
        {
            use project::operations::remap_index_after_move;
            self.test_parallel.operation_moved(from, to);
            if let Some(batch) = self.batch_calculation.as_mut() {
                batch.remap_after_move(from, to);
            }
            self.project.operations.move_item(from, to);
            if let project::selection::Selection::Operation(index) = self.project.selection {
                self.project.selection = project::selection::Selection::Operation(
                    remap_index_after_move(index, from, to),
                );
            }
            self.nc_export_ui.operation_reordered();
            self.nc_batch_export.invalidate();
            if let Some(command) = self.pending_cam_command {
                self.pending_cam_command = command
                    .index()
                    .map(|index| command.with_index(remap_index_after_move(index, from, to)));
            }
        }
        if self.project.model_transform != before_transform {
            self.section_profiles = None;
            self.sample_profiles = None;
            self.sample_profile_job = None;
            self.test_parallel
                .sync_tools(&mut self.project, &self.tool_library);
            self.message = "Transformacion STL aplicada: bruto reajustado y CAM invalidado; revisar setup y area de corte".into();
        }
        if let Some(command) = self.panels.take_operation_command() {
            self.request_cam_command(command, ui.ctx());
        }
        if let Some(index) = self.panels.take_edit_operation() {
            self.test_parallel.open_editor(index, &self.project);
        }
        self.test_parallel.show_editor(
            ui.ctx(),
            &mut self.project,
            self.current_stl_path.as_deref(),
            &self.tool_library,
            enabled,
        );
        if open_docked || open_floating {
            self.file_dialog(ui.ctx().clone());
        }
        self.panels.save_if_needed(ui.ctx());
        if ui.ctx().input(|i| i.viewport().close_requested())
            && !self.exit_approved
            && self.is_dirty()
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.ask_unsaved {
                self.pending_action = Some(FileAction::Exit);
                self.ask_unsaved = true;
            }
        }
        self.unsaved_dialog(ui.ctx());
        self.tool_ui.show(
            ui.ctx(),
            &mut self.tool_library,
            self.tool_library_path.as_deref(),
        );
        self.test_parallel
            .sync_tools(&mut self.project, &self.tool_library);
        self.nc_export_ui
            .refresh_verification(&self.project, &self.tool_library);
        self.nc_export_ui.show(
            ui.ctx(),
            &self.project,
            &self.test_parallel,
            &self.post_ui,
            &self.machine_ui,
        );
        self.nc_batch_export.show(
            ui.ctx(),
            &self.project,
            &self.test_parallel,
            &self.post_ui,
            &self.machine_ui,
        );
        if let Some((text, xyz, tool_signature)) = self.nc_export_ui.take_playback_request() {
            self.nc_playback_ui
                .load_generated(text, xyz, tool_signature);
        }
        if !self.nc_playback_ui.compact_active() {
            self.nc_playback_ui
                .show(ui.ctx(), &self.project, &self.tool_library);
        }
        self.post_ui.show(ui.ctx());
        self.machine_ui.show(ui.ctx());
        self.options.show(ui.ctx());
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.panels.save();
        self.cancel.store(true, Ordering::Relaxed);
        self.receiving = None;
        if let Some(gl) = gl {
            self.nc_playback_ui.destroy_scene_gl(gl);
            self.cam_marker_renderer.lock().destroy(gl);
            self.stock_renderer.lock().destroy(gl);
            self.renderer.lock().destroy(gl);
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        depth_buffer: 24,
        multisampling: 4,
        viewport: egui::ViewportBuilder::default()
            .with_title("LaguCAM - STL Viewer")
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "LaguCAM - STL Viewer",
        options,
        Box::new(|cc| {
            App::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| std::io::Error::other(e).into())
        }),
    )
}

#[cfg(test)]
mod pivot_press_tests {
    use super::PivotPress;
    use eframe::egui;

    #[test]
    fn buffers_drag_until_hit_is_resolved() {
        let mut press = PivotPress::default();
        press.start();
        assert_eq!(press.motion(egui::vec2(3.0, -1.0)), None);
        assert_eq!(press.motion(egui::vec2(2.0, 4.0)), None);
        assert_eq!(press.complete(), egui::vec2(5.0, 3.0));
        assert_eq!(
            press.motion(egui::vec2(1.0, 2.0)),
            Some(egui::vec2(1.0, 2.0))
        );
    }

    #[test]
    fn cancellation_drops_old_motion_before_next_press() {
        let mut press = PivotPress::default();
        press.start();
        assert_eq!(press.motion(egui::vec2(12.0, 8.0)), None);
        press = PivotPress::default();
        press.start();
        assert_eq!(press.complete(), egui::Vec2::ZERO);
    }

    #[test]
    fn marker_follows_press_release_without_forgetting_pivot_request() {
        let mut press = PivotPress::default();
        assert!(!press.marker_visible());
        press.start();
        assert!(!press.marker_visible()); // Aun esperando la superficie de GL.
        assert_eq!(press.complete(), egui::Vec2::ZERO);
        assert!(press.marker_visible());
        press.sync_held(false);
        assert!(!press.marker_visible());
        press.start();
        assert!(press.waiting_for_hit);
        press.sync_held(false);
        assert!(press.waiting_for_hit); // El clic sigue eligiendo pivote al soltar.
        assert!(!press.marker_visible());
    }

    #[test]
    fn default_action_bindings_match_existing_mouse_controls() {
        let bindings = super::NavigationBindings::default();
        assert_eq!(bindings.orbit, egui::PointerButton::Primary);
        assert_eq!(
            bindings.pan,
            [egui::PointerButton::Secondary, egui::PointerButton::Middle]
        );
    }
}

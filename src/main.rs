#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod overlay;
mod preview;
mod project;
mod project_ui;
mod render;
mod stl;
mod stock_render;

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

#[derive(Clone, Copy)]
enum FileAction {
    New,
    OpenProject,
    OpenStl,
    Exit,
}

struct App {
    renderer: Arc<Mutex<render::Render3d>>,
    stock_renderer: Arc<Mutex<stock_render::StockRenderer>>,
    project: project::Project,
    property_error: String,
    project_path: Option<PathBuf>,
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
        let mut app = Self {
            renderer: Arc::new(Mutex::new(mesh_renderer)),
            stock_renderer: Arc::new(Mutex::new(stock_renderer)),
            project: project::Project::default(),
            property_error: String::new(),
            project_path: None,
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
        self.opening_project = opening;
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
                    self.project_path = Some(opening.path);
                    self.message = match opening.warning {
                        Some(warning) => {
                            format!("Proyecto recuperado: {}. {warning}", self.filename)
                        }
                        None => format!("Proyecto recuperado correctamente: {}", self.filename),
                    };
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
                        // Se valida antes de finish_upload(); la malla anterior sigue activa.
                        let validation_error = self.opening_project.as_ref().and_then(|opening| {
                            opening.document.check_loaded(info).err().or_else(|| {
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
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn center_view(&mut self) {
        self.yaw = 45f32.to_radians();
        self.pitch = 35f32.to_radians();
        self.distance = 1.0;
        self.pan = glam::Vec3::ZERO;
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
                self.renderer.lock().clear_scene();
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
        let result = (|| -> Result<(), String> {
            let doc = persistence::read_project(&path)?;
            let Some(mut stl_path) = doc.resolve_stl(&path) else {
                let restored = doc.state.restore(String::new(), None)?;
                self.renderer.lock().clear_scene();
                self.project = restored;
                self.project_path = Some(path.clone());
                self.current_stl_path = None;
                self.saved_state = Some(persistence::ProjectState::capture(&self.project));
                self.filename = "Ningún STL abierto".into();
                self.property_error.clear();
                self.message = format!("Proyecto abierto: {}", path.display());
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
            ui.menu_button("Vista", |ui| {
                if ui.button("Centrar vista").clicked() {
                    self.center_view();
                    ui.ctx().request_repaint();
                    ui.close();
                }
            });
        });
    }
    // LaguCAM 003: panel limpio. El arbol y sus propiedades son la vista principal.
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.busy(), |ui| {
            project_ui::show(ui, &mut self.project, &mut self.property_error);
        });
        if let Some(path) = &self.project_path {
            ui.small(format!("Proyecto: {}", path.display()));
        }
        if self.is_dirty() {
            ui.colored_label(egui::Color32::YELLOW, "Cambios sin guardar");
        }
        // Progreso y cancelacion solo mientras se importa. Se conserva el flujo STL original.
        if self.busy() {
            ui.separator();
            if let Some(name) = &self.pending_filename {
                ui.label(format!("Importando: {name}"));
            }
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
            let fraction = self.progress.load(Ordering::Relaxed) as f32 / 100.0;
            ui.add(egui::ProgressBar::new(fraction.min(0.99)).show_percentage());
            ui.label(&self.message);
        } else if self.message.starts_with("ERROR") {
            // No ocultar errores de importacion ni de GPU tras limpiar el panel.
            ui.separator();
            ui.colored_label(egui::Color32::RED, &self.message);
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        if size.x <= 5.0 || size.y <= 5.0 {
            return;
        }
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        if response.dragged_by(egui::PointerButton::Primary) {
            let drag = response.drag_motion();
            self.yaw -= drag.x * 0.008;
            self.pitch = (self.pitch + drag.y * 0.008).clamp(-1.45, 1.45);
            ui.ctx().request_repaint();
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            let drag = response.drag_motion();
            let direction = glam::Vec3::new(
                self.pitch.cos() * self.yaw.cos(),
                self.pitch.cos() * self.yaw.sin(),
                self.pitch.sin(),
            );
            let right = glam::Vec3::Z.cross(direction).normalize();
            let up = direction.cross(right);
            let distance = render::fit_distance(rect.width() / rect.height()) * self.distance;
            let units = 2.0 * distance * (22.5f32.to_radians()).tan() / rect.height();
            self.pan += (-right * drag.x + up * drag.y) * units;
            ui.ctx().request_repaint();
        }
        if response.hovered() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.distance = (self.distance * (-scroll * 0.003).exp()).clamp(0.05, 20.0);
                ui.ctx().request_repaint();
            }
        }
        let yaw = self.yaw;
        let pitch = self.pitch;
        let distance = self.distance;
        let pan = self.pan;
        let aspect = rect.width() / rect.height().max(1.0);
        let model_color = self.project.model_color;
        let renderer = Arc::clone(&self.renderer);
        let stock_renderer = Arc::clone(&self.stock_renderer);
        let stock_geometry = stock_render::StockGeometry::from_project(&self.project);
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
                    mesh.paint(painter.gl(), yaw, pitch, distance, aspect, pan);
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
            },
            response.clicked(),
            response.hover_pos(),
        );
        ui.painter().text(
            rect.left_top() + egui::vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            "VISTA 3D  ·  Rotar: arrastrar  ·  Zoom: rueda  ·  Desplazar: botón derecho",
            egui::FontId::monospace(12.0),
            egui::Color32::LIGHT_GRAY,
        );
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());
        egui::Panel::top("main_menu").show(ui, |ui| self.menu(ui));
        // SidePanel y CentralPanel ocupan toda la altura disponible, a diferencia
        // de un horizontal cuyo alto se ajustaba al contenido de la barra lateral.
        egui::Panel::left("stl_sidebar")
            .resizable(true)
            .default_size(290.0)
            .min_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.sidebar(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| self.viewport(ui));
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
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.cancel.store(true, Ordering::Relaxed);
        self.receiving = None;
        if let Some(gl) = gl {
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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod overlay;
mod preview;
mod project;
mod project_ui;
mod render;
mod stl;

use eframe::{egui, egui_glow, glow};
use egui::mutex::Mutex;
use preview::LoadEvent;
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

struct App {
    renderer: Arc<Mutex<render::Render3d>>,
    project: project::Project,
    property_error: String,
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
        let mut app = Self {
            renderer: Arc::new(Mutex::new(render::Render3d::new(gl)?)),
            project: project::Project::default(),
            property_error: String::new(),
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
        if self.busy() {
            return;
        }
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
            self.message = format!("ERROR GPU: {error}. Se conserva el modelo anterior.");
        }
        if completed {
            if let Some(success) = self.pending_success.take() {
                self.filename = self.pending_filename.take().unwrap_or_default();
                self.project.loaded(self.filename.clone(), success.info);
                self.property_error.clear();
                self.message = format!(
                    "STL cargado completamente (lectura y preparación CPU: {:.1} s)",
                    success.time_seconds
                );
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
                        if self.cancel.load(Ordering::Relaxed) {
                            self.renderer.lock().abort_upload();
                            self.pending_filename = None;
                            self.message = stl::CANCELLED.into();
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
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("STL binario", &["stl"])
            .pick_file()
        {
            self.open(path, ctx);
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Archivo", |ui| {
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("Abrir STL…"))
                    .clicked()
                {
                    ui.close();
                    self.file_dialog(ui.ctx().clone());
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
        project_ui::show(ui, &mut self.project, &mut self.property_error);
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
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
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
        let renderer = Arc::clone(&self.renderer);
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
                renderer
                    .lock()
                    .paint(painter.gl(), yaw, pitch, distance, aspect, pan);
            })),
        });
        overlay::paint(ui, rect, &self.project, yaw, pitch, distance, pan);
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
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.cancel.store(true, Ordering::Relaxed);
        self.receiving = None;
        if let Some(gl) = gl {
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

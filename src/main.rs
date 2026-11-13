#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod preview;
mod render;
mod stl;

use eframe::{egui, egui_glow, glow};
use egui::mutex::Mutex;
use preview::Loaded;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

struct LoadStats {
    info: stl::StlInfo,
}

struct App {
    renderer: Arc<Mutex<render::Render3d>>,
    receiving: Option<Receiver<Result<Loaded, String>>>,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    pending_filename: Option<String>,
    filename: String,
    message: String,
    result: Option<LoadStats>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: glam::Vec3,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let gl = cc
            .gl
            .as_ref()
            .ok_or("Esta aplicación necesita el backend OpenGL")?;
        let mut app = Self {
            renderer: Arc::new(Mutex::new(render::Render3d::new(gl)?)),
            receiving: None,
            progress: Arc::new(AtomicU32::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
            pending_filename: None,
            filename: "Ningún STL abierto".to_owned(),
            message: "Selecciona un STL binario o abre el ejemplo incluido.".into(),
            result: None,
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

    fn open(&mut self, path: PathBuf, ctx: egui::Context) {
        if self.receiving.is_some() {
            return;
        }
        self.pending_filename = Some(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );
        self.message = "Leyendo STL y preparando una única malla visual...".into();
        self.progress.store(0, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let progress = Arc::clone(&self.progress);
        let cancel = Arc::clone(&self.cancel);
        self.receiving = Some(rx);
        let worker = std::thread::Builder::new()
            .name("stl-import".into())
            .spawn(move || {
                let result = preview::load(&path, &progress, &cancel);
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        if let Err(error) = worker {
            self.receiving = None;
            self.pending_filename = None;
            self.message = format!("No se pudo iniciar la importación: {error}");
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(error) = self.renderer.lock().take_error() {
            self.message = format!("ERROR GPU: {error}");
            self.result = None;
            self.filename = "Sin malla visible".into();
        }
        if let Some(rx) = &self.receiving {
            match rx.try_recv() {
                Ok(Ok(loaded)) => {
                    self.message =
                        format!("Previsualización preparada en {:.1} s", loaded.time_seconds);
                    if self.cancel.load(Ordering::Relaxed) {
                        self.message = stl::CANCELLED.into();
                        self.receiving = None;
                        self.pending_filename = None;
                        return;
                    }
                    self.filename = self.pending_filename.take().unwrap_or_default();
                    self.result = Some(LoadStats { info: loaded.info });
                    self.renderer.lock().replace_mesh(loaded.mesh);
                    self.receiving = None;
                    self.yaw = 45f32.to_radians();
                    self.pitch = 35f32.to_radians();
                    self.distance = 1.0;
                    self.pan = glam::Vec3::ZERO;
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    self.message = if error == stl::CANCELLED {
                        error
                    } else {
                        format!("ERROR: {error}")
                    };
                    self.receiving = None;
                    self.pending_filename = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.message = "ERROR: se interrumpió la lectura".into();
                    self.receiving = None;
                    self.pending_filename = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }
    }

    fn center_view(&mut self) {
        self.yaw = 45f32.to_radians();
        self.pitch = 35f32.to_radians();
        self.distance = 1.0;
        self.pan = glam::Vec3::ZERO;
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Archivo", |ui| {
                if ui
                    .add_enabled(self.receiving.is_none(), egui::Button::new("Abrir STL…"))
                    .clicked()
                {
                    ui.close();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("STL binario", &["stl"])
                        .pick_file()
                    {
                        self.open(path, ui.ctx().clone());
                    }
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

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("CAM Libre · Visor 0.1");
        ui.separator();
        ui.label("STL binario · vista completa");
        if ui
            .add_enabled(self.receiving.is_none(), egui::Button::new("Abrir STL..."))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("STL binario", &["stl"])
                .pick_file()
        {
            self.open(path, ui.ctx().clone());
        }
        ui.add_space(10.0);
        ui.label(egui::RichText::new(&self.filename).strong());
        if self.receiving.is_some() {
            if let Some(name) = &self.pending_filename {
                ui.label(format!("Importando: {name}"));
            }
            if ui.button("Cancelar importación").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
            }
            let fraction = self.progress.load(Ordering::Relaxed) as f32 / 100.0;
            ui.add(egui::ProgressBar::new(fraction).show_percentage());
        }
        ui.label(&self.message);
        if let Some(result) = &self.result {
            ui.separator();
            ui.label(format!("Triángulos STL: {}", result.info.triangles));
            ui.label(format!(
                "Archivo: {:.1} MiB",
                result.info.size_bytes as f64 / 1_048_576.0
            ));
            let d = result.info.bounds.dimensions();
            ui.label(format!("Extensión X: {:.2}", d[0]));
            ui.label(format!("Extensión Y: {:.2}", d[1]));
            ui.label(format!("Extensión Z: {:.2}", d[2]));
            ui.small("Unidades heredadas del STL (STL no define unidades).");
            ui.add_space(8.0);
            ui.label("Vista completa, sin simplificación de caras");
        }
        ui.separator();
        if ui.button("Centrar vista").clicked() {
            self.center_view();
        }
        ui.add_space(12.0);
        ui.label("Controles");
        ui.small("Botón izquierdo: girar");
        ui.small("Rueda: acercar / alejar");
        ui.small("Botón derecho o central: desplazar");
        ui.small("Coordenadas GPU normalizadas a f32. El STL original permanece intacto.");
        ui.add_space(10.0);
        ui.colored_label(
            egui::Color32::from_rgb(220, 175, 100),
            "No utilizar para calcular G-code.",
        );
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
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                let height = ui.available_height();
                ui.allocate_ui_with_layout(
                    egui::vec2(260.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.sidebar(ui),
                );
                ui.separator();
                self.viewport(ui);
            });
        });
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.cancel.store(true, Ordering::Relaxed);
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
            .with_title("CAM Libre - STL Viewer")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "CAM Libre - STL Viewer",
        options,
        Box::new(|cc| {
            App::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| std::io::Error::other(e).into())
        }),
    )
}

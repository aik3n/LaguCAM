//! P2-C2: reproduccion visual del programa NC FINAL, no de PlannedToolpath.
//! El nucleo de interpretacion es P2-C1; aqui solo hay tiempo visual, UI y dibujo.
//! La velocidad es movimientos/segundo, NO tiempo real de la maquina.
//! El cursor se interpola entre endpoints del NC interpretado; no cambia el STL.

use crate::{
    material_ui::MaterialUi,
    navigation::Projection,
    project::{Project, cam_coordinates::WorkFrame},
};
use cam_libre_visor::{
    nc_contract::{
        NcDistanceMode, NcEventKind, NcInitialState, NcMotion, NcMotionKind, NcSpindleMode,
        NcTrace, NcUnits,
    },
    nc_interpreter::interpret_program,
};
use eframe::egui;
use std::{fs, path::Path, time::Instant};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VISIBLE_SEGMENTS: usize = 12_000;

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

fn next_tick(playhead: f64, elapsed_secs: f64, speed_moves_sec: f64, total: usize) -> f64 {
    // Acotar delta tras un paron de UI evita saltos involuntarios en la reproduccion.
    let delta = elapsed_secs.clamp(0.0, 0.1) * speed_moves_sec;
    (playhead + delta).min(total as f64)
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

struct Playback {
    trace: NcTrace,
    moves: Vec<DisplayMotion>,
    frame: WorkFrame,
    model_name: String,
    model_triangles: u32,
    model_size_bytes: u64,
    model_bounds: crate::stl::Bounds,
}

pub struct NcPlaybackUi {
    pub open: bool,
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
    material: MaterialUi,
    message: String,
}

impl Default for NcPlaybackUi {
    fn default() -> Self {
        Self {
            open: false,
            nc_text: String::new(),
            source_name: String::new(),
            proposed_start: None,
            initial_xyz: [0.0; 3],
            initial_confirmed: false,
            playback: None,
            playhead: 0.0,
            playing: false,
            speed: 5.0,
            last_update: None,
            visible: true,
            material: MaterialUi::default(),
            message: String::new(),
        }
    }
}

impl NcPlaybackUi {
    pub fn clear(&mut self) {
        self.material.clear();
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

    /// Recibe exactamente la salida textual del postprocesador, no el camino CAM.
    /// La posicion sugerida NO se adopta automaticamente: Start podria mover la herramienta.
    pub fn load_generated(&mut self, text: String, first_cam_position: [f64; 3]) {
        self.clear();
        self.nc_text = text;
        self.source_name = "Vista previa NC de LaguCAM".into();
        self.proposed_start = Some(first_cam_position);
        self.open = true;
        self.message =
            "Indica y confirma la posicion real de la herramienta ANTES de Start.".into();
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
        let model = project.model.as_ref().ok_or("No hay STL cargado")?;
        self.playback = Some(Playback {
            trace,
            moves,
            frame,
            model_name: model.name.clone(),
            model_triangles: model.triangles,
            model_size_bytes: model.size_bytes,
            model_bounds: model.bounds,
        });
        self.playing = false;
        self.last_update = None;
        self.playhead = 0.0;
        Ok(())
    }

    fn projection_current(&self, project: &Project) -> bool {
        let Some(playback) = &self.playback else {
            return false;
        };
        let Some(model) = &project.model else {
            return false;
        };
        model.name == playback.model_name
            && model.triangles == playback.model_triangles
            && model.size_bytes == playback.model_size_bytes
            && model.bounds.min == playback.model_bounds.min
            && model.bounds.max == playback.model_bounds.max
            && Self::frame(project).is_ok_and(|frame| frame == playback.frame)
    }

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
        if self.playback.is_some() && !self.projection_current(project) {
            self.material.clear();
            self.playback = None;
            self.playing = false;
            self.last_update = None;
            self.message = "Modelo, bruto u origen modificados: vuelve a interpretar el NC".into();
        }
        let now = Instant::now();
        if self.playing {
            if let Some(pb) = &self.playback {
                let secs = self.last_update.map_or(0.0, |last| {
                    now.saturating_duration_since(last).as_secs_f64()
                });
                self.playhead = next_tick(self.playhead, secs, self.speed, pb.moves.len());
                if self.playhead >= pb.moves.len() as f64 {
                    self.playing = false;
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }
            }
        }
        self.last_update = Some(now);
        let mut open = self.open;
        let mut load_file = false;
        let mut interpret = false;
        let mut copy_proposal = false;
        let mut change_initial = false;
        egui::Window::new("Reproduccion NC (G-code interpretado)")
            .open(&mut open)
            .default_size([570.0, 510.0])
            .min_width(460.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Abrir archivo NC...").clicked() { load_file = true; }
                    ui.label(&self.source_name);
                });
                ui.colored_label(egui::Color32::YELLOW,
                    "Reproduccion geometrica; NO verifica colisiones, aceleraciones ni seguridad de maquina.");
                ui.separator();
                ui.label("Posicion inicial de la punta de herramienta, en coordenadas de trabajo (mm), ANTES de Start:");
                ui.horizontal(|ui| {
                    for axis in 0..3 {
                        ui.label(["X", "Y", "Z"][axis]);
                        if ui.add(egui::DragValue::new(&mut self.initial_xyz[axis]).speed(0.1).max_decimals(6)).changed() {
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
                if ui.checkbox(&mut self.initial_confirmed,
                    "He comprobado la posicion inicial y entiendo que no se deduce del G-code").changed() {
                    change_initial = true;
                }
                if ui.button("Interpretar NC y preparar trayectoria").clicked() {
                    interpret = true;
                }
                if let Some(pb) = &self.playback {
                    ui.separator();
                    ui.label(format!("{} movimientos NC · {} eventos interpretados · {} lineas NC",
                        pb.moves.len(), pb.trace.events.len(), self.nc_text.lines().count()));
                    if pb.moves.is_empty() {
                        ui.label("El archivo no contiene desplazamientos XYZ.");
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button(if self.playing { "Pausar" } else { "Reproducir" }).clicked() {
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
                                self.playhead = (self.playhead.floor() + 1.0).min(pb.moves.len() as f64);
                            }
                        });
                        ui.add(egui::Slider::new(&mut self.playhead, 0.0..=pb.moves.len() as f64)
                            .text("Movimiento NC"));
                        ui.add(egui::Slider::new(&mut self.speed, 0.25..=60.0)
                            .logarithmic(true).text("Movimientos/s (visual)"));
                        let i = self.playhead.floor() as usize;
                        let line = pb.moves.get(i).or_else(|| pb.moves.last()).map_or(0, |m| m.line);
                        let xyz = position_at(pb.trace.initial.position_work_mm, &pb.moves, self.playhead);
                        ui.monospace(format!("Linea NC: {line}  ·  X{:.4} Y{:.4} Z{:.4} mm", xyz[0], xyz[1], xyz[2]));
                    }
                    ui.checkbox(&mut self.visible, "Mostrar trayectoria NC en visor 3D");
                    ui.small("G0 en cian; G1 en naranja. El marcador representa la punta, no la geometria completa de la fresa.");
                    if pb.moves.len() > MAX_VISIBLE_SEGMENTS {
                        ui.small("Vista de trazos simplificada por rendimiento; reproduccion y posicion conservan todos los movimientos.");
                    }
                }
                if let Some(pb) = &self.playback {
                    self.material.show(ui, project, tool_library, &pb.trace, self.playhead);
                }
                if !self.message.is_empty() {
                    ui.separator();
                    ui.label(&self.message);
                }
            });
        self.open = open;
        if copy_proposal {
            if let Some(suggested) = self.proposed_start {
                self.initial_xyz = suggested;
                self.initial_confirmed = false;
                change_initial = true;
            }
        }
        if change_initial {
            self.playing = false;
            self.last_update = None;
            self.playback = None;
            self.material.clear();
        }
        if load_file {
            if let Err(e) = self.load_file() {
                self.message = format!("ERROR: {e}");
            }
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
        self.material.paint(
            ui,
            rect,
            project,
            self.playhead,
            yaw,
            pitch,
            distance,
            pan,
            projection,
        );
        if !self.visible && !self.material.isolated_view_active() {
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
            if let Some(current) = pb.moves.get(index) {
                if let (Some(a), Some(b)) = (
                    point(current.motion.from_work_mm),
                    point(current.motion.to_work_mm),
                ) {
                    painter.line_segment([a, b], egui::Stroke::new(2.8, egui::Color32::WHITE));
                }
            }
        }
        let tip = position_at(pb.trace.initial.position_work_mm, &pb.moves, self.playhead);
        if let Some(pos) = point(tip) {
            painter.circle_filled(pos, 6.0, egui::Color32::from_rgb(235, 55, 62));
            painter.circle_stroke(pos, 8.0, egui::Stroke::new(1.6, egui::Color32::WHITE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn playback_speed_is_visual_and_capped_after_long_pause() {
        assert_eq!(next_tick(2.0, 0.01, 10.0, 5), 2.1);
        assert_eq!(next_tick(2.0, 600.0, 10.0, 5), 3.0);
        assert_eq!(next_tick(4.9, 0.02, 10.0, 5), 5.0);
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

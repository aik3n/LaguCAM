//! Ensayo visual OpenGL aislado del visor principal.
//! Simula una pasada con el motor real y sube la malla completa 1001x1001 una sola vez.
#[path = "diagnostico_opengl_v3_support/nc_contract.rs"]
mod nc_contract;
#[path = "diagnostico_opengl_v3_support/material_sim.rs"]
mod material_sim;

use material_sim::{
    CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox,
};
use eframe::{egui, egui_glow, glow};
use glam::{Mat4, Vec3};
use glow::HasContext as _;
use std::{
    sync::{Arc, atomic::{AtomicBool, AtomicUsize}},
    time::Instant,
};

const N: usize = 2001;
const EXTENT: f64 = 100.0;
const CUT_Z: f64 = 5.0;
const RADIUS: f64 = 2.5;
const COLOR_CUT: [f32; 3] = [0.18, 0.63, 0.78];
const COLOR_UNCUT: [f32; 3] = [0.70, 0.49, 0.30];

struct Mesh {
    vertices: Vec<f32>, // position normalized + normal + color, 9 f32
    indices: Vec<u32>,
    profile: Vec<(f64, f64)>,
    error_mm: f64,
    seconds: f64,
    changes: usize,
}

fn build(kind: CutterShape) -> Result<Mesh, String> {
    let stock = StockBox {
        min_mm: [0.0; 3],
        size_mm: [EXTENT, EXTENT, 10.0],
    };
    // Nueve pasadas paralelas, paso lateral 2 mm; se conserva el mismo Z de punta.
    let segments: Vec<CutSegment> = (0..9).map(|i| {
        let y = 42.0 + 2.0 * i as f64;
        let (x0, x1) = if i % 2 == 0 { (20.0, 80.0) } else { (80.0, 20.0) };
        CutSegment {
            from_model_mm: [x0, y, CUT_Z],
            to_model_mm: [x1, y, CUT_Z],
            spindle_on: true,
            rapid: false,
            nc_line: i + 1,
        }
    }).collect();
    let began = Instant::now();
    let mut field = MaterialHeightfield::simulate_with_budget(
        stock,
        CutterSpec { shape: kind, diameter_mm: RADIUS * 2.0, cutting_length_mm: 12.0 },
        N,
        512,
        &segments,
        &AtomicBool::new(false),
        &AtomicUsize::new(0),
    )?;
    field.seek(segments.len() as f64);
    let seconds = began.elapsed().as_secs_f64();
    let changes = field.history_len();

    let z = |x: usize, y: usize| field.height_at(x, y);
    let mut vertices = Vec::with_capacity(N * N * 9);
    for j in 0..N {
        for i in 0..N {
            let i0 = i.saturating_sub(1);
            let i1 = (i + 1).min(N - 1);
            let j0 = j.saturating_sub(1);
            let j1 = (j + 1).min(N - 1);
            let dzdx = (z(i1, j) - z(i0, j)) / ((i1 - i0) as f64 * 0.05);
            let dzdy = (z(i, j1) - z(i, j0)) / ((j1 - j0) as f64 * 0.05);
            let normal = Vec3::new(-(dzdx as f32), -(dzdy as f32), 1.0).normalize_or_zero();
            let color = if field.is_cut(i, j) { COLOR_CUT } else { COLOR_UNCUT };
            // Normalizacion uniforme de XYZ: altura vertical fisicamente coherente.
            let x = i as f32 * 0.001 - 1.0;
            let y = j as f32 * 0.001 - 1.0;
            let zz = z(i, j) as f32 * 0.02 - 0.1;
            vertices.extend_from_slice(&[x, y, zz, normal.x, normal.y, normal.z]);
            vertices.extend_from_slice(&color);
        }
    }
    let mut indices = Vec::with_capacity((N - 1) * (N - 1) * 6);
    for j in 0..N - 1 {
        for i in 0..N - 1 {
            let a = (j * N + i) as u32;
            let b = a + 1;
            let c = a + N as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }
    let mut profile = Vec::new();
    let mut error_mm: f64 = 0.0;
    // Perfil transversal por el centro del recorrido: incluye la forma de la huella.
    for j in 900..=1100 {
        let y = j as f64 * 0.05;
        // La huella es la envolvente inferior de las nueve herramientas.
        let dy = (0..9)
            .map(|i| (y - (42.0 + 2.0 * i as f64)).abs())
            .fold(f64::INFINITY, f64::min);
        let expected = if dy <= RADIUS {
            match kind {
                CutterShape::Flat => CUT_Z,
                CutterShape::Ball => CUT_Z + RADIUS - (RADIUS * RADIUS - dy * dy).max(0.0).sqrt(),
            }
        } else { 10.0 };
        let actual = z(1000, j);
        error_mm = error_mm.max((actual - expected).abs());
        profile.push((y, actual));
    }
    Ok(Mesh { vertices, indices, profile, error_mm, seconds, changes })
}

struct GlRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    u_mvp: glow::UniformLocation,
    uploaded: usize,
    count: i32,
}
impl GlRenderer {
    fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vertex = gl.create_shader(glow::VERTEX_SHADER)?;
            gl.shader_source(vertex, r#"#version 330 core
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_normal;
layout(location=2) in vec3 a_color;
uniform mat4 u_mvp;
out vec3 v_normal;
out vec3 v_color;
void main() {
    gl_Position = u_mvp * vec4(a_pos,1.0);
    v_normal = a_normal;
    v_color = a_color;
}"#);
            gl.compile_shader(vertex);
            if !gl.get_shader_compile_status(vertex) {
                return Err(format!("Vertex shader: {}", gl.get_shader_info_log(vertex)));
            }
            let fragment = gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(fragment, r#"#version 330 core
in vec3 v_normal;
in vec3 v_color;
out vec4 result;
void main() {
    vec3 n = normalize(v_normal);
    vec3 light = normalize(vec3(-0.35,-0.45,0.82));
    float diffuse = max(dot(n,light),0.0);
    result = vec4(v_color*(0.35+0.65*diffuse),1.0);
}"#);
            gl.compile_shader(fragment);
            if !gl.get_shader_compile_status(fragment) {
                return Err(format!("Fragment shader: {}", gl.get_shader_info_log(fragment)));
            }
            gl.attach_shader(program,vertex);
            gl.attach_shader(program,fragment);
            gl.link_program(program);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !gl.get_program_link_status(program) {
                return Err(format!("Link OpenGL: {}", gl.get_program_info_log(program)));
            }
            let u_mvp = gl.get_uniform_location(program,"u_mvp").ok_or("Uniform u_mvp ausente")?;
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            let ebo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            let stride = 9*4;
            for (location,offset) in [(0,0),(1,12),(2,24)] {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(location,3,glow::FLOAT,false,stride,offset);
            }
            gl.bind_vertex_array(None);
            Ok(Self { program,vao,vbo,ebo,u_mvp,uploaded:usize::MAX,count:0 })
        }
    }
    fn paint(&mut self, gl: &glow::Context, mesh: &Mesh, selected: usize, mvp: Mat4) {
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp),false,&mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            if self.uploaded != selected {
                gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&mesh.vertices),glow::STATIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(self.ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&mesh.indices),glow::STATIC_DRAW);
                self.count = mesh.indices.len() as i32;
                self.uploaded = selected;
            }
            gl.draw_elements(glow::TRIANGLES,self.count,glow::UNSIGNED_INT,0);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA,glow::ONE_MINUS_SRC_ALPHA);
        }
    }
}

struct DiagnosticApp {
    meshes: [Arc<Mesh>; 2],
    selected: usize,
    yaw: f32,
    pitch: f32,
    distance: f32,
    frame_clock: Instant,
    fps_average: f64,
    cache: Arc<egui::mutex::Mutex<Option<Result<GlRenderer,String>>>>,
}
impl DiagnosticApp {
    fn new(meshes: [Arc<Mesh>;2]) -> Self {
        Self {
            meshes, selected:0, yaw:0.7, pitch:0.42,distance:2.6,
            frame_clock:Instant::now(), fps_average:0.0,
            cache: Arc::new(egui::mutex::Mutex::new(None)),
        }
    }
}
impl eframe::App for DiagnosticApp {
    fn ui(&mut self,ui:&mut egui::Ui,_frame:&mut eframe::Frame) {
        let ctx=ui.ctx().clone();
        let now=Instant::now();
        let dt=now.duration_since(self.frame_clock).as_secs_f64();
        self.frame_clock=now;
        if dt>0.0001 && dt<1.0 {
            let fps=1.0/dt;
            if self.fps_average==0.0 {self.fps_average=fps;}
            else {self.fps_average=self.fps_average*0.94+fps*0.06;}
        }
        // Fuerza refresco para comparar tasas de fotogramas con escena estatica.
        ctx.request_repaint();
        egui::Panel::top("controls").show(ui,|ui|{
            ui.heading("LaguCAM | Prueba rendimiento OpenGL 0,05 mm");
            ui.horizontal(|ui|{
                ui.selectable_value(&mut self.selected,0,"Fresa plana Ø5");
                ui.selectable_value(&mut self.selected,1,"Fresa esferica Ø5");
                if ui.button("Reiniciar camara").clicked() {
                    self.yaw=0.7;self.pitch=0.42;self.distance=2.6;
                }
            });
            let mesh = &self.meshes[self.selected];
            ui.label(format!(
                "Malla {} x {} (sin decimacion) | Calculo {:.3} s | Cambios {} | \
                Desviacion maxima del perfil analitico {:.4} mm",
                N,N,mesh.seconds,mesh.changes,mesh.error_mm
            ));
            ui.label(format!("FPS interfaz (media movil): {:.1} | triangulos: {:.1} millones | \
                VBO+EBO aprox. {:.0} MiB por malla",
                self.fps_average,mesh.indices.len() as f64/3.0/1_000_000.0,
                (mesh.vertices.len()*4+mesh.indices.len()*4) as f64/1_048_576.0));
            ui.small("9 pasadas: X20..80, Y42..58 (paso 2 mm), punta Z5; bruto 100 x 100 x 10 mm. \
            2001x2001, sin decimacion ni exageracion vertical. Arrastra para girar; rueda para zoom.");
        });
        egui::Panel::bottom("profile").resizable(false).show(ui,|ui|{
            ui.label("Perfil X50, Y45..55 mm; grafica con escala Z ampliada para distinguir crestas (NO afecta al 3D)");
            let desired=egui::vec2(ui.available_width(),108.0);
            let (rect,_) = ui.allocate_exact_size(desired,egui::Sense::hover());
            let painter=ui.painter_at(rect);
            painter.rect_filled(rect,0.0,egui::Color32::from_gray(28));
            let profile=&self.meshes[self.selected].profile;
            for pair in profile.windows(2) {
                let cv=|v:(f64,f64)| egui::pos2(
                    rect.left() + ((v.0-45.0)/10.0) as f32*rect.width(),
                    rect.bottom() - ((v.1-4.95)/0.4) as f32*rect.height()
                );
                painter.line_segment([cv(pair[0]),cv(pair[1])],egui::Stroke::new(1.5,egui::Color32::LIGHT_GREEN));
            }
        });
        egui::CentralPanel::default().show(ui,|ui|{
            let (rect,response)=ui.allocate_exact_size(ui.available_size(),egui::Sense::drag());
            if response.dragged() {
                let delta=ctx.input(|i| i.pointer.delta());
                self.yaw+=delta.x*0.008;
                self.pitch=(self.pitch+delta.y*0.008).clamp(0.08,1.48);
                ctx.request_repaint();
            }
            if response.hovered() {
                let scroll=ctx.input(|i| i.smooth_scroll_delta.y);
                if scroll!=0.0 {
                    self.distance=(self.distance*(-scroll*0.001).exp()).clamp(1.7,10.0);
                    ctx.request_repaint();
                }
            }
            let eye=Vec3::new(
                self.distance*self.pitch.cos()*self.yaw.cos(),
                self.distance*self.pitch.cos()*self.yaw.sin(),
                self.distance*self.pitch.sin(),
            );
            let aspect=(rect.width()/rect.height().max(1.0)).max(0.01);
            let mvp=Mat4::perspective_rh_gl(45.0_f32.to_radians(),aspect,0.01,50.0)
                * Mat4::look_at_rh(eye,Vec3::ZERO,Vec3::Z);
            let mesh=Arc::clone(&self.meshes[self.selected]);
            let cache=Arc::clone(&self.cache);
            let selected=self.selected;
            ui.painter().add(egui::PaintCallback {
                rect,
                callback:Arc::new(egui_glow::CallbackFn::new(move |info,painter|{
                    let gl=painter.gl();
                    let clip=egui::PaintCallbackInfo {
                        clip_rect:info.clip_rect.intersect(info.viewport),
                        ..info
                    }.clip_rect_in_pixels();
                    unsafe {
                        gl.enable(glow::SCISSOR_TEST);
                        gl.scissor(clip.left_px,clip.from_bottom_px,clip.width_px,clip.height_px);
                        gl.clear(glow::DEPTH_BUFFER_BIT);
                        gl.disable(glow::SCISSOR_TEST);
                    }
                    let mut lock=cache.lock();
                    if lock.is_none() { *lock=Some(GlRenderer::new(gl)); }
                    if let Some(Ok(renderer))=lock.as_mut() {
                        renderer.paint(gl,&mesh,selected,mvp);
                    }
                })),
            });
        });
    }
}

fn main()->Result<(),Box<dyn std::error::Error>>{
    let flat=Arc::new(build(CutterShape::Flat)?);
    let ball=Arc::new(build(CutterShape::Ball)?);
    println!("Prueba OpenGL 2001 x 2001 (~8 millones de triangulos)");
    println!("VBO ~144 MiB + EBO ~92 MiB por malla; temporales CPU adicionales");
    println!("Plana: perfil error max {:.5} mm, calculo {:.3} s",flat.error_mm,flat.seconds);
    println!("Esferica: perfil error max {:.5} mm, calculo {:.3} s",ball.error_mm,ball.seconds);
    let options=eframe::NativeOptions::default();
    eframe::run_native(
        "LaguCAM - Rendimiento OpenGL 0,05 mm",
        options,
        Box::new(move |_cc| Ok(Box::new(DiagnosticApp::new([flat,ball])))),
    )?;
    Ok(())
}
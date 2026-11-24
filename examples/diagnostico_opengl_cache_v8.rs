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
    fn paint(&mut self, gl: &glow::Context, reference: &Mesh, dynamic:&mut TemporalMesh, selected:usize, mvp:Mat4, draw_mode:usize) {
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp),false,&mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.vbo));
            if self.uploaded != selected {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&dynamic.vertices),glow::DYNAMIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(self.ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&reference.indices),glow::STATIC_DRAW);
                self.count=reference.indices.len() as i32;
                self.uploaded=selected;
                dynamic.dirty=None;
            } else if let Some((low,high))=dynamic.dirty.take() {
                let started=Instant::now();
                let pitch=EXTENT as f32/(N-1) as f32;
                let x0=((20.0_f32-RADIUS as f32-pitch)/pitch).floor().max(0.0) as usize;
                let x1=(((80.0_f32+RADIUS as f32+pitch)/pitch).ceil() as usize).min(N-1);
                for row in low..=high {
                    let begin=(row*N+x0)*9;
                    let end=(row*N+x1+1)*9;
                    gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER,(begin*4) as i32,
                        bytemuck::cast_slice(&dynamic.vertices[begin..end]));
                }
                if let Ok(mut value)=dynamic.upload_ms.lock() {
                    *value=started.elapsed().as_secs_f64()*1000.0;
                }
            }
            // Solo diagnostico: prefijos de los indices, sin reducir malla almacenada.
            // No es LOD, no debe incorporarse en el producto.
            let visible_indices = match draw_mode {
                1 => self.count,
                2 => (self.count / 4 / 3) * 3,
                3 => (self.count / 16 / 3) * 3,
                _ => 0,
            };
            if visible_indices > 0 {
                gl.draw_elements(glow::TRIANGLES,visible_indices,glow::UNSIGNED_INT,0);
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA,glow::ONE_MINUS_SRC_ALPHA);
        }
    }
}

// Superficie mutable de prueba. Replica solo el barrido recto del ensayo;
// al finalizar se compara con la geometria del motor real de material.
struct TemporalMesh {
    vertices: Vec<f32>,
    dirty: Option<(usize, usize)>, // filas inclusivas, rango X fijo para el ensayo
    upload_ms: Arc<std::sync::Mutex<f64>>,
    pass: usize,
    kind: CutterShape,
    last_bytes: usize,
    cpu_ms: f64,
}
impl TemporalMesh {
    fn new(final_mesh: &Mesh, kind: CutterShape) -> Self {
        let mut vertices = final_mesh.vertices.clone();
        for v in vertices.chunks_exact_mut(9) {
            v[2] = 10.0_f32 * 0.02 - 0.1;
            v[3] = 0.0; v[4] = 0.0; v[5] = 1.0;
            v[6..9].copy_from_slice(&COLOR_UNCUT);
        }
        Self {vertices,dirty:Some((0,N-1)),pass:0,kind,last_bytes:0,
              cpu_ms:0.0,upload_ms:Arc::new(std::sync::Mutex::new(0.0))}
    }
    fn reset(&mut self) {
        for v in self.vertices.chunks_exact_mut(9) {
            v[2]=0.1;v[3]=0.0;v[4]=0.0;v[5]=1.0;
            v[6..9].copy_from_slice(&COLOR_UNCUT);
        }
        self.pass=0;
        self.cpu_ms=0.0;
        self.last_bytes=0;
        self.dirty=Some((0,N-1));
    }
    fn step(&mut self) {
        if self.pass>=9 {return;}
        let step = EXTENT as f32 / (N-1) as f32;
        let center_y=42.0_f32 + 2.0*self.pass as f32;
        let y0=(((center_y-3.0)/step).floor() as usize).min(N-1);
        let y1=(((center_y+3.0)/step).ceil() as usize).min(N-1);
        let start=Instant::now();
        for j in y0..=y1 {
            let y=j as f32*step;
            for i in 0..N {
                let x=i as f32*step;
                let along=x.clamp(20.0,80.0);
                let dx=x-along;
                let dy=y-center_y;
                let r2=dx*dx+dy*dy;
                if r2 > (RADIUS as f32)*(RADIUS as f32) {continue;}
                let bottom=match self.kind {
                    CutterShape::Flat=>0.0_f32,
                    CutterShape::Ball=>RADIUS as f32-((RADIUS as f32).powi(2)-r2).max(0.0).sqrt(),
                };
                let desired=(CUT_Z as f32+bottom)*0.02-0.1;
                let v=&mut self.vertices[(j*N+i)*9..(j*N+i+1)*9];
                if desired<v[2] {
                    v[2]=desired;
                    v[6..9].copy_from_slice(&COLOR_CUT);
                }
            }
        }
        // Normales locales; 1 fila de margen alrededor de la franja alterada.
        let low=y0.saturating_sub(1);
        let high=(y1+1).min(N-1);
        // Solo se recalculan normales del rectangulo que puede cambiar por la fresa.
        let x0=((20.0_f32 - RADIUS as f32 - step) / step).floor().max(0.0) as usize;
        let x1=(((80.0_f32 + RADIUS as f32 + step) / step).ceil() as usize).min(N-1);
        for j in low..=high {
            for i in x0..=x1 {
                let a=i.saturating_sub(1);
                let b=(i+1).min(N-1);
                let c=j.saturating_sub(1);
                let d=(j+1).min(N-1);
                let dzdx=(self.vertices[(j*N+b)*9+2]-self.vertices[(j*N+a)*9+2])
                    / ((b-a) as f32*step*0.02);
                let dzdy=(self.vertices[(d*N+i)*9+2]-self.vertices[(c*N+i)*9+2])
                    / ((d-c) as f32*step*0.02);
                let normal=Vec3::new(-dzdx,-dzdy,1.0).normalize_or_zero();
                let v=&mut self.vertices[(j*N+i)*9..(j*N+i+1)*9];
                v[3]=normal.x;v[4]=normal.y;v[5]=normal.z;
            }
        }
        self.pass+=1;
        self.last_bytes=(high-low+1)*(x1-x0+1)*9*4;
        self.dirty=Some(match self.dirty {Some((a,b))=>(a.min(low),b.max(high)),None=>(low,high)});
        self.cpu_ms=start.elapsed().as_secs_f64()*1000.0;
        println!("Pasada {}: CPU {:.3} ms; transferencia prevista {:.2} MiB",
            self.pass,self.cpu_ms,self.last_bytes as f64/1048576.0);
    }
    fn max_error(&self, reference:&Mesh)->f32 {
        self.vertices.chunks_exact(9).zip(reference.vertices.chunks_exact(9))
            .map(|(a,b)| ((a[2]-b[2])/0.02).abs()).fold(0.0,f32::max)
    }
}

struct DiagnosticApp {
    meshes:[Arc<Mesh>;2],
    temporal: Arc<egui::mutex::Mutex<[TemporalMesh;2]>>,
    selected:usize,
    draw_mode:usize,
    refresh_mode:usize,
    ui_ms:f64,
    ui_ms_peak:f64,
    final_errors:[Option<f32>;2],
    frame_ms:f64,
    yaw:f32,
    pitch:f32,
    distance:f32,
    playing:bool,
    last_step:Instant,
    frame_clock:Instant,
    fps_average:f64,
    cache:Arc<egui::mutex::Mutex<Option<Result<GlRenderer,String>>>>,
}
impl DiagnosticApp {
    fn new(meshes:[Arc<Mesh>;2])->Self {
        let temporal=[TemporalMesh::new(&meshes[0],CutterShape::Flat),
                      TemporalMesh::new(&meshes[1],CutterShape::Ball)];
        Self {meshes,temporal:Arc::new(egui::mutex::Mutex::new(temporal)),
              selected:1,draw_mode:0,refresh_mode:0,ui_ms:0.0,ui_ms_peak:0.0,final_errors:[None,None],frame_ms:0.0,yaw:0.7,pitch:0.42,distance:2.6,playing:false,
              last_step:Instant::now(),frame_clock:Instant::now(),fps_average:0.0,
              cache:Arc::new(egui::mutex::Mutex::new(None))}
    }
}
impl eframe::App for DiagnosticApp {
    fn ui(&mut self,ui:&mut egui::Ui,_frame:&mut eframe::Frame) {
        let started_ui=Instant::now();
        let ctx=ui.ctx().clone();
        let now=Instant::now();
        let dt=now.duration_since(self.frame_clock).as_secs_f64();
        self.frame_clock=now;
        self.frame_ms=dt*1000.0;
        if (0.0001..1.0).contains(&dt) {
            let fps=1.0/dt;
            self.fps_average=if self.fps_average==0.0 {fps} else {0.94*self.fps_average+0.06*fps};
        }
        if self.playing && now.duration_since(self.last_step).as_secs_f64()>=1.0 {
            self.last_step=now;
            let mut both=self.temporal.lock();
            both[0].step();both[1].step();
            if both[0].pass==9 {self.playing=false;}
        }
        match self.refresh_mode {
            0 => ctx.request_repaint(),
            1 => ctx.request_repaint_after(std::time::Duration::from_millis(16)),
            2 => ctx.request_repaint_after(std::time::Duration::from_millis(33)),
            // En modo eventos, el valor de FPS deja de representar rendimiento maximo.
            _ => if self.playing {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            },
        }
        egui::Panel::top("controls").show(ui,|ui| {
            ui.heading("LaguCAM | Diagnostico cuello de botella OpenGL V6 (0,05 mm)");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected,0,"Fresa plana Ø5");
                ui.selectable_value(&mut self.selected,1,"Fresa esferica Ø5");
                if ui.button(if self.playing {"Pausar"} else {"Reproducir"}).clicked(){
                    self.playing=!self.playing;self.last_step=Instant::now();
                }
                if ui.button("Siguiente pasada").clicked() {
                    self.playing=false;
                    let mut both=self.temporal.lock();
                    both[0].step();both[1].step();
                }
                if ui.button("Reiniciar").clicked() {
                    self.playing=false;
                    let mut both=self.temporal.lock();
                    both[0].reset();both[1].reset();
                    self.final_errors=[None,None];
                }
                if ui.button("Reiniciar camara").clicked() {
                    self.yaw=0.7;self.pitch=0.42;self.distance=2.6;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Solicitud repintado:");
                ui.selectable_value(&mut self.refresh_mode,0,"Inmediato");
                ui.selectable_value(&mut self.refresh_mode,1,"Tras 16 ms");
                ui.selectable_value(&mut self.refresh_mode,2,"Tras 33 ms");
                ui.selectable_value(&mut self.refresh_mode,3,"Solo eventos");
            });
            ui.horizontal(|ui| {
                ui.label("Carga de dibujo:");
                ui.selectable_value(&mut self.draw_mode,0,"Sin dibujar");
                ui.selectable_value(&mut self.draw_mode,3,"~0,5 M triangulos");
                ui.selectable_value(&mut self.draw_mode,2,"~2 M triangulos");
                ui.selectable_value(&mut self.draw_mode,1,"~8 M triangulos");
            });
            ui.label(format!(
                "Intervalo entre llamadas {:.2} ms | UI Rust anterior {:.2} ms (pico {:.2}) | \
                FPS aparente {:.1}",
                self.frame_ms,self.ui_ms,self.ui_ms_peak,self.fps_average
            ));
            let both=self.temporal.lock();
            let temporal=&both[self.selected];
            let upload_ms=temporal.upload_ms.lock().map(|v|*v).unwrap_or(-1.0);
            ui.label(format!("Pasada {}/9 | FPS interfaz {:.1} | CPU ultima pasada {:.2} ms | \
                Subida OpenGL ultima pasada {:.2} ms | Subida aprox. {:.2} MiB",
                temporal.pass,self.fps_average,temporal.cpu_ms,upload_ms,
                temporal.last_bytes as f64/1048576.0));
            if temporal.pass==9 {
                // Se calcula una sola vez al finalizar las pasadas.
                let error=match self.final_errors[self.selected] {
                    Some(value)=>value,
                    None=>{
                        let value=temporal.max_error(&self.meshes[self.selected]);
                        self.final_errors[self.selected]=Some(value);
                        value
                    }
                };
                ui.label(format!("Diferencia maxima frente al motor real (perfil final): {:.5} mm",error));
            }
            ui.small("V7: comparar Inmediato / Tras 16 ms / Tras 33 ms con SIN DIBUJAR. \
                UI Rust mide construccion del interfaz, NO callback OpenGL, composicion o espera de presentacion. \
                El modo Solo eventos no sirve para comparar FPS. Sin sincronizacion GPU explicita.");
        });
        egui::CentralPanel::default().show(ui,|ui| {
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
            let eye=Vec3::new(self.distance*self.pitch.cos()*self.yaw.cos(),
                    self.distance*self.pitch.cos()*self.yaw.sin(),self.distance*self.pitch.sin());
            let aspect=(rect.width()/rect.height().max(1.0)).max(0.01);
            let mvp=Mat4::perspective_rh_gl(45.0_f32.to_radians(),aspect,0.01,50.0)
                *Mat4::look_at_rh(eye,Vec3::ZERO,Vec3::Z);
            let reference=Arc::clone(&self.meshes[self.selected]);
            let selected=self.selected;
            let draw_mode=self.draw_mode;
            let temporal=Arc::clone(&self.temporal);
            let cache=Arc::clone(&self.cache);
            ui.painter().add(egui::PaintCallback {
                rect,
                callback:Arc::new(egui_glow::CallbackFn::new(move |info,painter| {
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
                    // No preparar ni subir buffers en modo sin dibujo.
                    if draw_mode==0 { return; }
                    let mut cache=cache.lock();
                    if cache.is_none() {*cache=Some(GlRenderer::new(gl));}
                    if let Some(Ok(renderer))=cache.as_mut(){
                        let mut both=temporal.lock();
                        renderer.paint(gl,&reference,&mut both[selected],selected,mvp,draw_mode);
                    }
                })),
            });
        });
        self.ui_ms=started_ui.elapsed().as_secs_f64()*1000.0;
        self.ui_ms_peak=self.ui_ms_peak.max(self.ui_ms);
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
        "LaguCAM - V8 comparacion en cache",
        options,
        Box::new(move |_cc| Ok(Box::new(DiagnosticApp::new([flat,ball])))),
    )?;
    Ok(())
}
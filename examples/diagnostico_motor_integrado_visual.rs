//! LaguCAM V13: experimento interactivo NC + bloques COW + OpenGL.
//! Un solo bruto 100x100x10 mm; 60 pasadas, paso XY 0.05 mm.
//! Reproduccion por indice de segmento: NO temporizacion de maquina real.
mod engine {
    use cam_libre_visor::material_sim::{CutterShape, CutterSpec, StockBox, CutSegment, segments_from_trace};
    use cam_libre_visor::nc_contract::{NcDistanceMode, NcInitialState, NcSpindleMode, NcUnits};
    use cam_libre_visor::nc_interpreter::interpret_program;
    use cam_libre_visor::sparse_material::{SparseMaterial, SparseConfig};

    pub struct Demo { segments: Vec<CutSegment>, material: SparseMaterial }
    impl Demo {
        pub fn new() -> Result<Self,String> {
            let initial = NcInitialState {
                position_work_mm: [0.0,0.0,12.0], units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute, feed_mm_min: None,
                spindle_rpm: None, spindle_mode: NcSpindleMode::Stopped, tool_number: None,
            };
            let mut nc=String::from("G21 G90 G94\nG0 X20 Y20 Z12\nS12000 M3\nF600\n");
            for i in 0..60 {
                let y=20.0 + i as f64 * 0.8;
                let (a,b) = if i%2==0 {(20.0,45.0)} else {(45.0,20.0)};
                if i>0 { nc.push_str("G0 Z12\n"); nc.push_str(&format!("G0 X{a:.3} Y{y:.3}\n")); }
                nc.push_str(&format!("G1 Z5\nG1 X{b:.3} Y{y:.3}\n"));
            }
            nc.push_str("G0 Z12\nM5\nG0 X0 Y0 Z12\nM30\n");
            let trace=interpret_program(&nc,initial).map_err(|e|e.to_string())?;
            let segments=segments_from_trace(&trace,Ok)?;
            let material=SparseMaterial::new(SparseConfig {
                stock: StockBox {min_mm:[0.0;3],size_mm:[100.0,100.0,10.0]},
                cutter: CutterSpec {shape:CutterShape::Ball,diameter_mm:2.0,cutting_length_mm:12.0},
                spacing_mm:0.05,checkpoint_every:25,max_checkpoints:4,height_budget_mib:64,
            })?;
            Ok(Self {segments,material})
        }
        pub fn len(&self)->usize {self.segments.len()}
        pub fn seek(&mut self,position:f64)->Result<(),String>{self.material.seek(&self.segments,position)}
        pub fn height(&self,x:usize,y:usize)->f32 {self.material.height_at(x,y).unwrap_or(10.0) as f32}
        pub fn tiles(&self)->usize {self.material.stats().tiles}
        pub fn memory_mib(&self)->f64 {self.material.stats().height_bytes as f64/1048576.0}
        pub fn checkpoints(&self)->usize {self.material.stats().checkpoints}
        pub fn replayed(&self)->usize {self.material.stats().replayed_moves}
    }
}

use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use glam::{Mat4, Vec3};
use std::{sync::Arc, time::{Duration, Instant}};

const N: usize = 2001;
const STRIDE: usize = 9;
const STEP: f32 = 0.05;
const LIGHT: [f32;3]=[0.70,0.49,0.30];
const CUT: [f32;3]=[0.18,0.63,0.78];
const MAX_MIB: f64=64.0;

struct SurfaceMesh {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    dirty: Option<(usize,usize)>,
    total_upload_bytes: usize,
}
impl SurfaceMesh {
    fn new()->Self {
        let mut vertices=Vec::with_capacity(N*N*STRIDE);
        for j in 0..N {
            for i in 0..N {
                vertices.extend_from_slice(&[
                    i as f32*0.001-1.0,j as f32*0.001-1.0,0.1,
                    0.0,0.0,1.0,LIGHT[0],LIGHT[1],LIGHT[2]
                ]);
            }
        }
        let mut indices=Vec::with_capacity((N-1)*(N-1)*6);
        for j in 0..N-1 { for i in 0..N-1 {
            let a=(j*N+i) as u32;
            indices.extend_from_slice(&[a,a+1,a+N as u32+1,a,a+N as u32+1,a+N as u32]);
        }}
        Self{vertices,indices,dirty:Some((0,N-1)),total_upload_bytes:0}
    }
    // Reconstruir SOLO rectangulo de fresado, incluyendo margen de normales.
    // La superficie restante conserva sus datos en GPU. El motor mantiene 0.05mm en todo XY.
    fn refresh(&mut self,demo:&engine::Demo) {
        let x0=350usize;let x1=940usize;
        let y0=350usize;let y1=1440usize;
        for j in y0..=y1 { for i in x0..=x1 {
            let index=(j*N+i)*STRIDE;
            let z=demo.height(i,j);
            self.vertices[index+2]=z*0.02-0.1;
            let color=if z < 9.999999 {CUT} else {LIGHT};
            self.vertices[index+6..index+9].copy_from_slice(&color);
        }}
        for j in y0..=y1 { for i in x0..=x1 {
            let z=|x:usize,y:usize| self.vertices[(y*N+x)*STRIDE+2];
            let a=i.saturating_sub(1);let b=(i+1).min(N-1);
            let c=j.saturating_sub(1);let d=(j+1).min(N-1);
            let dzdx=(z(b,j)-z(a,j))/((b-a) as f32*STEP*0.02);
            let dzdy=(z(i,d)-z(i,c))/((d-c) as f32*STEP*0.02);
            let normal=Vec3::new(-dzdx,-dzdy,1.0).normalize_or_zero();
            let index=(j*N+i)*STRIDE;
            self.vertices[index+3..index+6].copy_from_slice(&normal.to_array());
        }}
        self.dirty=Some(match self.dirty { Some((a,b))=>(a.min(y0),b.max(y1)), None=>(y0,y1) });
    }
}

struct Renderer {
    program:glow::Program, vao:glow::VertexArray,vbo:glow::Buffer,ebo:glow::Buffer,
    uniform:glow::UniformLocation,shade_uniform:glow::UniformLocation,uploaded:bool,
}
impl Renderer {
    fn new(gl:&glow::Context)->Result<Self,String>{
        unsafe {
            let program=gl.create_program()?;
            let vs=gl.create_shader(glow::VERTEX_SHADER)?;
            gl.shader_source(vs,r#"#version 330 core
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_normal;
layout(location=2) in vec3 a_color;
uniform mat4 mvp;
out vec3 n;
out vec3 c;
out vec3 p;
void main(){gl_Position=mvp*vec4(a_pos,1.0);n=a_normal;c=a_color;p=a_pos;}"#);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs){return Err(gl.get_shader_info_log(vs));}
            let fs=gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(fs,r#"#version 330 core
in vec3 n;
in vec3 c;
in vec3 p;
uniform int shading_mode;
out vec4 frag;
void main(){
    vec3 normal=n;
    if(shading_mode==1){
        normal=normalize(cross(dFdx(p),dFdy(p)));
        if(normal.z<0.0) normal=-normal;
    }
    float light=max(dot(normalize(normal),normalize(vec3(-0.35,-0.45,0.82))),0.0);
    frag=vec4(c*(0.35+0.65*light),1.0);
}"#);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs){return Err(gl.get_shader_info_log(fs));}
            gl.attach_shader(program,vs);gl.attach_shader(program,fs);gl.link_program(program);
            gl.delete_shader(vs);gl.delete_shader(fs);
            if !gl.get_program_link_status(program){return Err(gl.get_program_info_log(program));}
            let uniform=gl.get_uniform_location(program,"mvp").ok_or("Uniform mvp ausente")?;
            let shade_uniform=gl.get_uniform_location(program,"shading_mode").ok_or("Uniform shading_mode ausente")?;
            let vao=gl.create_vertex_array()?;let vbo=gl.create_buffer()?;let ebo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            for (location,offset) in [(0,0),(1,12),(2,24)] {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(location,3,glow::FLOAT,false,36,offset);
            }
            gl.bind_vertex_array(None);
            Ok(Self{program,vao,vbo,ebo,uniform,shade_uniform,uploaded:false})
        }
    }
    fn paint(&mut self,gl:&glow::Context,mesh:&mut SurfaceMesh,mvp:Mat4,draw:bool,faceted:bool){
        unsafe {
            gl.enable(glow::DEPTH_TEST);gl.depth_func(glow::LEQUAL);gl.depth_mask(true);
            gl.disable(glow::BLEND);gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.uniform),false,&mvp.to_cols_array());
            gl.uniform_1_i32(Some(&self.shade_uniform), if faceted {1} else {0});
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.vbo));
            if !self.uploaded {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&mesh.vertices),glow::DYNAMIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(self.ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&mesh.indices),glow::STATIC_DRAW);
                self.uploaded=true;
                mesh.total_upload_bytes+=mesh.vertices.len()*4+mesh.indices.len()*4;
                mesh.dirty=None;
            } else if let Some((low,high))=mesh.dirty.take() {
                // Solo XY que puede cambiar: X17.5..47; Y17.5..72 mm.
                let x0=350usize;let x1=940usize;
                for row in low..=high {
                    let a=(row*N+x0)*STRIDE;let b=(row*N+x1+1)*STRIDE;
                    gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER,(a*4) as i32,
                        bytemuck::cast_slice(&mesh.vertices[a..b]));
                    mesh.total_upload_bytes+=(b-a)*4;
                }
            }
            if draw {gl.draw_elements(glow::TRIANGLES,mesh.indices.len() as i32,glow::UNSIGNED_INT,0);}
            gl.bind_vertex_array(None);gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA,glow::ONE_MINUS_SRC_ALPHA);
        }
    }
}

struct App {
    demo:engine::Demo,
    mesh:Arc<egui::mutex::Mutex<SurfaceMesh>>,
    renderer:Arc<egui::mutex::Mutex<Option<Result<Renderer,String>>>>,
    current:f64,playing:bool,draw:bool,faceted:bool,yaw:f32,pitch:f32,distance:f32,
    tick:Instant, clock:Instant, fps:f64,seek_ms:f64, mesh_ms:f64,
    message:String,
}
impl App {
    fn new()->Result<Self,String>{
        let demo=engine::Demo::new()?;
        Ok(Self{demo,mesh:Arc::new(egui::mutex::Mutex::new(SurfaceMesh::new())),
            renderer:Arc::new(egui::mutex::Mutex::new(None)),
            current:0.0,playing:false,draw:true,faceted:true,yaw:0.7,pitch:0.42,distance:2.6,
            tick:Instant::now(),clock:Instant::now(),fps:0.0,seek_ms:0.0,mesh_ms:0.0,
            message:String::new()})
    }
    fn seek(&mut self,position:f64){
        let position=position.clamp(0.0,self.demo.len() as f64);
        if (position-self.current).abs()<1.0e-8{return;}
        let started=Instant::now();
        match self.demo.seek(position){
            Ok(())=>{
                self.seek_ms=started.elapsed().as_secs_f64()*1000.0;
                let started=Instant::now();
                self.mesh.lock().refresh(&self.demo);
                self.mesh_ms=started.elapsed().as_secs_f64()*1000.0;
                self.current=position;
                self.message.clear();
            },
            Err(e)=>{self.playing=false;self.message=e;}
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self,ui:&mut egui::Ui,_frame:&mut eframe::Frame){
        let ctx=ui.ctx().clone();
        let now=Instant::now();
        let dt=now.duration_since(self.clock).as_secs_f64();self.clock=now;
        if dt>0.0001 && dt<1.0 {let sample=1.0/dt;self.fps=if self.fps==0.0{sample}else{0.94*self.fps+0.06*sample};}
        if self.playing && now.duration_since(self.tick)>Duration::from_millis(300){
            self.tick=now;
            if self.current>=self.demo.len() as f64{self.playing=false;}
            else {self.seek((self.current.floor()+1.0).min(self.demo.len() as f64));}
        }
        if self.playing {ctx.request_repaint_after(Duration::from_millis(16));}
        egui::Panel::top("controls").show(ui,|ui|{
            ui.heading("LaguCAM — motor integrado: prueba de mecanizado y sombreado");
            ui.horizontal(|ui|{
                if ui.button(if self.playing{"Pausar"}else{"Reproducir"}).clicked(){self.playing=!self.playing;self.tick=Instant::now();}
                if ui.button("Anterior").clicked(){self.playing=false;self.seek((self.current.ceil()-1.0).max(0.0));}
                if ui.button("Siguiente").clicked(){self.playing=false;self.seek((self.current.floor()+1.0).min(self.demo.len() as f64));}
                if ui.button("Inicio").clicked(){self.playing=false;self.seek(0.0);}
                if ui.button("Final").clicked(){self.playing=false;self.seek(self.demo.len() as f64);}
                ui.checkbox(&mut self.draw,"Dibujar superficie");
                ui.checkbox(&mut self.faceted,"Normales por cara (sin suavizado visual)");
            });
            let mut selection=self.current;
            if ui.add(egui::Slider::new(&mut selection,0.0..=self.demo.len() as f64).text("Posicion NC")).changed(){
                self.playing=false;self.seek(selection);
            }
            ui.label(format!("NC: {:.2}/{} movimientos | FPS {:.1} | Bloques {} | Memoria alturas {:.2} MiB | CP {} | Replay {} movimientos",
                self.current,self.demo.len(),self.fps,self.demo.tiles(),self.demo.memory_mib(),self.demo.checkpoints(),self.demo.replayed()));
            ui.label(format!("Seek CPU {:.2} ms | Actualizacion malla CPU {:.2} ms | Bytes GPU acumulados {:.1} MiB",
                self.seek_ms,self.mesh_ms,self.mesh.lock().total_upload_bytes as f64/1048576.0));
            ui.small("Usa src/sparse_material.rs REAL, resolucion 0,05 mm, fresa esferica D2. Alternar normales NO modifica alturas.\nReproduccion por segmento (300 ms), no tiempo fisico CNC. Arrastra para girar; rueda para zoom.");
            if !self.message.is_empty(){ui.colored_label(egui::Color32::RED,&self.message);}
        });
        egui::CentralPanel::default().show(ui,|ui|{
            let (rect,response)=ui.allocate_exact_size(ui.available_size(),egui::Sense::drag());
            if response.dragged(){let delta=ctx.input(|i|i.pointer.delta());self.yaw+=delta.x*0.008;self.pitch=(self.pitch+delta.y*0.008).clamp(0.08,1.48);}
            if response.hovered(){let scroll=ctx.input(|i|i.smooth_scroll_delta.y);if scroll!=0.0{self.distance=(self.distance*(-scroll*0.001).exp()).clamp(1.7,12.0);}}
            let eye=Vec3::new(self.distance*self.pitch.cos()*self.yaw.cos(),self.distance*self.pitch.cos()*self.yaw.sin(),self.distance*self.pitch.sin());
            let aspect=(rect.width()/rect.height().max(1.0)).max(0.01);
            let mvp=Mat4::perspective_rh_gl(45.0_f32.to_radians(),aspect,0.01,50.0)*Mat4::look_at_rh(eye,Vec3::ZERO,Vec3::Z);
            let mesh=Arc::clone(&self.mesh);let renderer=Arc::clone(&self.renderer);let draw=self.draw;let faceted=self.faceted;
            ui.painter().add(egui::PaintCallback{rect,callback:Arc::new(egui_glow::CallbackFn::new(move |info,painter|{
                let gl=painter.gl();
                let clip=egui::PaintCallbackInfo{clip_rect:info.clip_rect.intersect(info.viewport),..info}.clip_rect_in_pixels();
                unsafe{gl.enable(glow::SCISSOR_TEST);gl.scissor(clip.left_px,clip.from_bottom_px,clip.width_px,clip.height_px);gl.depth_mask(true);gl.clear(glow::DEPTH_BUFFER_BIT);gl.disable(glow::SCISSOR_TEST);}
                if !draw {return;}
                let mut guard=renderer.lock();
                if guard.is_none(){*guard=Some(Renderer::new(gl));}
                if let Some(Ok(gl_renderer))=guard.as_mut(){gl_renderer.paint(gl,&mut mesh.lock(),mvp,draw,faceted);}
            }))});
        });
    }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let app=App::new().map_err(std::io::Error::other)?;
    eframe::run_native("LaguCAM — Validacion visual del motor sparse_material",eframe::NativeOptions::default(),
        Box::new(move |_cc|Ok(Box::new(app))))?;
    Ok(())
}
//! LaguCAM V13: experimento interactivo NC + bloques COW + OpenGL.
//! Un solo bruto 100x100x10 mm; 60 pasadas, paso XY 0.05 mm.
//! Reproduccion por indice de segmento: NO temporizacion de maquina real.
#[path = "diagnostico_v14_support/engine.rs"]
#[allow(dead_code)]
mod engine;

use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use glam::{Mat4, Vec3};
use std::{sync::{Arc, mpsc::{self, SyncSender, Receiver, TrySendError}}, time::{Duration, Instant}};

const N: usize = 2001;
const STRIDE: usize = 9;
const STEP: f32 = 0.05;
const LIGHT: [f32;3]=[0.70,0.49,0.30];
const CUT: [f32;3]=[0.18,0.63,0.78];
const MAX_MIB: f64=64.0;

struct SurfaceMesh {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    dirty: Vec<(usize,usize,usize,usize)>, // rectangulos: x0,x1,y0,y1
    total_upload_bytes: usize,
    changed_tiles: usize,
    upload_ms:f64,
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
        Self{vertices,indices,dirty:Vec::new(),total_upload_bytes:0,changed_tiles:0,upload_ms:0.0}
    }
    // Los tiles proceden de diferencias reales entre estados del motor NC.
    // Z y color se actualizan primero; normales despues con un vertice de margen.
    fn refresh(&mut self,patches:&[engine::TilePatch]) {
        self.changed_tiles=patches.len();
        let mut regions=Vec::with_capacity(patches.len());
        for patch in patches {
            let x0=patch.tx*128;
            let y0=patch.ty*128;
            if x0>=N || y0>=N {continue;}
            let x1=((patch.tx+1)*128).min(N)-1;
            let y1=((patch.ty+1)*128).min(N)-1;
            for j in y0..=y1 {
                for i in x0..=x1 {
                    let z=patch.values.as_ref().map(|a|a[(j-y0)*128+(i-x0)]).unwrap_or(10.0);
                    let index=(j*N+i)*STRIDE;
                    self.vertices[index+2]=z*0.02-0.1;
                    self.vertices[index+6..index+9].copy_from_slice(if z<9.999999 {&CUT} else {&LIGHT});
                }
            }
            regions.push((x0.saturating_sub(1),(x1+1).min(N-1),
                          y0.saturating_sub(1),(y1+1).min(N-1)));
        }
        for &(x0,x1,y0,y1) in &regions {
            for j in y0..=y1 {for i in x0..=x1 {
                let z=|x:usize,y:usize|self.vertices[(y*N+x)*STRIDE+2];
                let a=i.saturating_sub(1);let b=(i+1).min(N-1);
                let c=j.saturating_sub(1);let d=(j+1).min(N-1);
                let dzdx=(z(b,j)-z(a,j))/((b-a) as f32*STEP*0.02);
                let dzdy=(z(i,d)-z(i,c))/((d-c) as f32*STEP*0.02);
                let n=Vec3::new(-dzdx,-dzdy,1.0).normalize_or_zero();
                self.vertices[(j*N+i)*STRIDE+3..(j*N+i)*STRIDE+6].copy_from_slice(&n.to_array());
            }}
        }
        self.dirty.extend(regions);
    }

}

struct Renderer {
    program:glow::Program, vao:glow::VertexArray,vbo:glow::Buffer,ebo:glow::Buffer,
    uniform:glow::UniformLocation,uploaded:bool,
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
void main(){gl_Position=mvp*vec4(a_pos,1.0);n=a_normal;c=a_color;}"#);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs){return Err(gl.get_shader_info_log(vs));}
            let fs=gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(fs,r#"#version 330 core
in vec3 n;
in vec3 c;
out vec4 frag;
void main(){float light=max(dot(normalize(n),normalize(vec3(-0.35,-0.45,0.82))),0.0);frag=vec4(c*(0.35+0.65*light),1.0);}"#);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs){return Err(gl.get_shader_info_log(fs));}
            gl.attach_shader(program,vs);gl.attach_shader(program,fs);gl.link_program(program);
            gl.delete_shader(vs);gl.delete_shader(fs);
            if !gl.get_program_link_status(program){return Err(gl.get_program_info_log(program));}
            let uniform=gl.get_uniform_location(program,"mvp").ok_or("Uniform mvp ausente")?;
            let vao=gl.create_vertex_array()?;let vbo=gl.create_buffer()?;let ebo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            for (location,offset) in [(0,0),(1,12),(2,24)] {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(location,3,glow::FLOAT,false,36,offset);
            }
            gl.bind_vertex_array(None);
            Ok(Self{program,vao,vbo,ebo,uniform,uploaded:false})
        }
    }
    fn paint(&mut self,gl:&glow::Context,mesh:&mut SurfaceMesh,mvp:Mat4,draw:bool){
        unsafe {
            gl.enable(glow::DEPTH_TEST);gl.depth_func(glow::LEQUAL);
            gl.disable(glow::BLEND);gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.uniform),false,&mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.vbo));
            if !self.uploaded {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&mesh.vertices),glow::DYNAMIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(self.ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&mesh.indices),glow::STATIC_DRAW);
                self.uploaded=true;
                mesh.total_upload_bytes+=mesh.vertices.len()*4+mesh.indices.len()*4;
                mesh.dirty.clear();
            } else if !mesh.dirty.is_empty() {
                let began=Instant::now();
                // Agrupamos tiles vecinos en la misma fila de bloques para
                // reducir llamadas GL; conservamos la malla de precision completa.
                let mut regions=std::mem::take(&mut mesh.dirty);
                regions.sort_unstable_by_key(|&(x0,_,y0,_)|(y0,x0));
                for (x0,x1,y0,y1) in regions {
                    for row in y0..=y1 {
                        let a=(row*N+x0)*STRIDE;
                        let b=(row*N+x1+1)*STRIDE;
                        gl.buffer_sub_data_u8_slice(
                            glow::ARRAY_BUFFER,(a*4) as i32,
                            bytemuck::cast_slice(&mesh.vertices[a..b]));
                        mesh.total_upload_bytes+=(b-a)*4;
                    }
                }
                mesh.upload_ms=began.elapsed().as_secs_f64()*1000.0;
            }
            if draw {gl.draw_elements(glow::TRIANGLES,mesh.indices.len() as i32,glow::UNSIGNED_INT,0);}
            gl.bind_vertex_array(None);gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA,glow::ONE_MINUS_SRC_ALPHA);
        }
    }
}


struct ResultState {
    state:Result<engine::State,String>,
    seek_ms:f64,
}
struct App {
    tx:SyncSender<f64>,
    rx:Receiver<ResultState>,
    mesh:Arc<egui::mutex::Mutex<SurfaceMesh>>,
    renderer:Arc<egui::mutex::Mutex<Option<Result<Renderer,String>>>>,
    total:usize,
    current:f64,
    desired:f64,
    queued:Option<f64>,
    busy:bool,
    playing:bool,
    draw:bool,
    yaw:f32,
    pitch:f32,
    distance:f32,
    tick:Instant,
    clock:Instant,
    fps:f64,
    seek_ms:f64,
    mesh_ms:f64,
    tiles:usize,
    memory_mib:f64,
    checkpoints:usize,
    replayed:usize,
    message:String,
}
impl App {
    fn new()->Result<Self,String>{
        let mut demo=engine::Demo::new()?;
        let total=demo.len();
        let (tx,command_rx)=mpsc::sync_channel::<f64>(1);
        let (result_tx,rx)=mpsc::channel::<ResultState>();
        std::thread::Builder::new().name("lagucam-v14-simulation".to_owned())
            .spawn(move ||{
                while let Ok(position)=command_rx.recv() {
                    let began=Instant::now();
                    let state=demo.seek(position);
                    let seek_ms=began.elapsed().as_secs_f64()*1000.0;
                    if result_tx.send(ResultState{state,seek_ms}).is_err(){break;}
                }
            }).map_err(|e|e.to_string())?;
        Ok(Self{
            tx,rx,total,mesh:Arc::new(egui::mutex::Mutex::new(SurfaceMesh::new())),
            renderer:Arc::new(egui::mutex::Mutex::new(None)),
            current:0.0,desired:0.0,queued:None,busy:false,playing:false,draw:true,
            yaw:0.7,pitch:0.42,distance:2.6,
            tick:Instant::now(),clock:Instant::now(),fps:0.0,
            seek_ms:0.0,mesh_ms:0.0,tiles:0,memory_mib:0.0,
            checkpoints:0,replayed:0,message:String::new(),
        })
    }
    fn request(&mut self,p:f64){
        self.desired=p.clamp(0.0,self.total as f64);
        self.queued=Some(self.desired);
    }
    fn poll(&mut self){
        while let Ok(done)=self.rx.try_recv(){
            self.busy=false;
            self.seek_ms=done.seek_ms;
            match done.state {
                Ok(state)=>{
                    let started=Instant::now();
                    self.mesh.lock().refresh(&state.patches);
                    self.mesh_ms=started.elapsed().as_secs_f64()*1000.0;
                    self.current=state.position;
                    self.tiles=state.tiles;
                    self.memory_mib=state.memory_mib;
                    self.checkpoints=state.checkpoints;
                    self.replayed=state.replayed;
                    self.message.clear();
                }
                Err(error)=>{self.playing=false;self.message=error;}
            }
        }
        if !self.busy {
            if let Some(target)=self.queued.take() {
                if (self.current-target).abs()>1.0e-8 {
                    match self.tx.try_send(target) {
                        Ok(())=>self.busy=true,
                        Err(TrySendError::Full(value))=>self.queued=Some(value),
                        Err(TrySendError::Disconnected(_))=>{
                            self.playing=false;
                            self.message="El hilo de simulacion ha terminado".to_owned();
                        }
                    }
                }
            }
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self,ui:&mut egui::Ui,_frame:&mut eframe::Frame){
        let ctx=ui.ctx().clone();
        let now=Instant::now();
        let dt=now.duration_since(self.clock).as_secs_f64();
        self.clock=now;
        if dt>0.0001 && dt<1.0 {
            let sample=1.0/dt;
            self.fps=if self.fps==0.0{sample}else{0.94*self.fps+0.06*sample};
        }
        self.poll();
        if self.playing && !self.busy && self.queued.is_none()
            && now.duration_since(self.tick)>Duration::from_millis(300) {
            self.tick=now;
            if self.current>=self.total as f64 {self.playing=false;}
            else {self.request((self.current.floor()+1.0).min(self.total as f64));}
        }
        // El trabajo se ejecuta en otro hilo; la UI solo sondea su resultado.
        if self.playing || self.busy || self.queued.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        egui::Panel::top("controls").show(ui,|ui|{
            ui.heading("LaguCAM V14 - NC por bloques, OpenGL parcial y seek asincrono");
            ui.horizontal(|ui|{
                if ui.button(if self.playing{"Pausar"}else{"Reproducir"}).clicked(){
                    self.playing=!self.playing;self.tick=Instant::now();
                }
                if ui.button("Anterior").clicked(){
                    self.playing=false;self.request((self.desired.ceil()-1.0).max(0.0));
                }
                if ui.button("Siguiente").clicked(){
                    self.playing=false;self.request((self.desired.floor()+1.0).min(self.total as f64));
                }
                if ui.button("Inicio").clicked(){self.playing=false;self.request(0.0);}
                if ui.button("Final").clicked(){self.playing=false;self.request(self.total as f64);}
                ui.checkbox(&mut self.draw,"Dibujar superficie");
            });
            let mut selection=self.desired;
            if ui.add(egui::Slider::new(&mut selection,0.0..=self.total as f64)
                .text("Posicion NC")).changed(){
                self.playing=false;self.request(selection);
            }
            ui.label(format!("NC confirmado {:.2}/{} | Solicitado {:.2} | FPS {:.1} | {}",
                self.current,self.total,self.desired,self.fps,
                if self.busy {"Procesando"} else {"Listo"}));
            ui.label(format!("Bloques {} | Alturas {:.2} MiB | CP {} | Replay {} movs | Tiles cambiados {}",
                self.tiles,self.memory_mib,self.checkpoints,self.replayed,
                self.mesh.lock().changed_tiles));
            let mesh=self.mesh.lock();
            ui.label(format!("Seek hilo {:.2} ms | Malla CPU {:.2} ms | Subida GL {:.2} ms | Transferencia acumulada {:.1} MiB",
                self.seek_ms,self.mesh_ms,mesh.upload_ms,mesh.total_upload_bytes as f64/1048576.0));
            drop(mesh);
            ui.small("V14: solo tiles modificados; calcula NC en hilo separado y aplica resultados ordenados. \
                No es reloj fisico: 1 movimiento cada 300ms. La malla OpenGL sigue siendo densa 2001x2001.");
            if !self.message.is_empty(){ui.colored_label(egui::Color32::RED,&self.message);}
        });
        egui::CentralPanel::default().show(ui,|ui|{
            let (rect,response)=ui.allocate_exact_size(ui.available_size(),egui::Sense::drag());
            if response.dragged(){
                let delta=ctx.input(|i|i.pointer.delta());
                self.yaw+=delta.x*0.008;
                self.pitch=(self.pitch+delta.y*0.008).clamp(0.08,1.48);
                ctx.request_repaint();
            }
            if response.hovered(){
                let scroll=ctx.input(|i|i.smooth_scroll_delta.y);
                if scroll!=0.0{
                    self.distance=(self.distance*(-scroll*0.001).exp()).clamp(1.7,12.0);
                    ctx.request_repaint();
                }
            }
            let eye=Vec3::new(self.distance*self.pitch.cos()*self.yaw.cos(),
                self.distance*self.pitch.cos()*self.yaw.sin(),self.distance*self.pitch.sin());
            let aspect=(rect.width()/rect.height().max(1.0)).max(0.01);
            let mvp=Mat4::perspective_rh_gl(45.0_f32.to_radians(),aspect,0.01,50.0)
                *Mat4::look_at_rh(eye,Vec3::ZERO,Vec3::Z);
            let mesh=Arc::clone(&self.mesh);
            let renderer=Arc::clone(&self.renderer);
            let draw=self.draw;
            ui.painter().add(egui::PaintCallback{
                rect,
                callback:Arc::new(egui_glow::CallbackFn::new(move |info,painter|{
                    let gl=painter.gl();
                    let clip=egui::PaintCallbackInfo{
                        clip_rect:info.clip_rect.intersect(info.viewport),..info
                    }.clip_rect_in_pixels();
                    unsafe{
                        gl.enable(glow::SCISSOR_TEST);
                        gl.scissor(clip.left_px,clip.from_bottom_px,clip.width_px,clip.height_px);
                        gl.clear(glow::DEPTH_BUFFER_BIT);
                        gl.disable(glow::SCISSOR_TEST);
                    }
                    if !draw{return;}
                    let mut guard=renderer.lock();
                    if guard.is_none(){*guard=Some(Renderer::new(gl));}
                    if let Some(Ok(gl_renderer))=guard.as_mut(){
                        gl_renderer.paint(gl,&mut mesh.lock(),mvp,draw);
                    }
                })),
            });
        });
    }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let app=App::new().map_err(std::io::Error::other)?;
    eframe::run_native("LaguCAM V14 - NC OpenGL parcial",eframe::NativeOptions::default(),
        Box::new(move |_cc|Ok(Box::new(app))))?;
    Ok(())
}
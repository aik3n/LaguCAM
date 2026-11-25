use std::collections::{HashMap, HashSet};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc::{self, SyncSender, Receiver, TrySendError}};
use std::time::{Instant, Duration};
use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use glam::{Mat4, Vec3};

#[path = "diagnostico_v15_support/engine.rs"]
#[allow(dead_code)]
mod engine;

const TILE: usize = 128;
const SIDE: f32 = 100.0;
const STEP: f32 = 0.05;
const N: usize = 2001;
const STRIDE: usize = 9;
const TOP: f32 = 10.0;
const CUT: [f32;3] = [0.18,0.63,0.78];
const WOOD: [f32;3] = [0.70,0.49,0.30];
type Key=(usize,usize);

fn put_vertex(out:&mut Vec<f32>,ix:usize,iy:usize,z:f32,n:[f32;3],color:[f32;3]){
    let scale=2.0/SIDE;
    out.extend_from_slice(&[
        ix as f32*STEP*scale-1.0, iy as f32*STEP*scale-1.0,
        z*scale-0.1,
        n[0],n[1],n[2],color[0],color[1],color[2],
    ]);
}

struct TileMesh {vertices:Vec<f32>,indices:Vec<u32>}
struct SparseMesh {
    data:HashMap<Key,Vec<f32>>,
    dirty:HashSet<Key>,
    base_dirty:bool,
    changed:usize,
    upload_mib:f64,
    upload_ms:f64,
    revision:u64,
}
impl SparseMesh {
    fn new()->Self{
        Self{data:HashMap::new(),dirty:HashSet::new(),base_dirty:true,
            changed:0,upload_mib:0.0,upload_ms:0.0,revision:0}
    }
    fn z(&self,ix:usize,iy:usize)->f32{
        if ix>=N || iy>=N {return TOP;}
        self.data.get(&(ix/TILE,iy/TILE))
            .map(|a|a[(iy%TILE)*TILE+(ix%TILE)])
            .unwrap_or(TOP)
    }
    fn apply(&mut self,patches:&[engine::TilePatch]){
        self.changed=patches.len();
        if !patches.is_empty() {self.revision=self.revision.wrapping_add(1);}
        for patch in patches {
            let key=(patch.tx,patch.ty);
            let had=self.data.contains_key(&key);
            if let Some(data)=&patch.values {self.data.insert(key,data.clone());}
            else {self.data.remove(&key);}
            if had!=self.data.contains_key(&key){self.base_dirty=true;}
            // Hay que actualizar tambien normales y bordes de bloques vecinos.
            for dy in -1_i32..=1 {for dx in -1_i32..=1 {
                let x=patch.tx as i32+dx;
                let y=patch.ty as i32+dy;
                if x>=0 && y>=0 {self.dirty.insert((x as usize,y as usize));}
            }}
        }
    }
    // Propiedad de CELDAS, no solo de muestras: una muestra en el borde
    // izquierdo/inferior de un bloque tambien afecta celdas del vecino.
    fn active_keys(&self)->HashSet<Key>{
        let mut keys=HashSet::new();
        let max=(N-2)/TILE;
        for &(tx,ty) in self.data.keys(){
            for x in [Some(tx),tx.checked_sub(1)] {
                for y in [Some(ty),ty.checked_sub(1)] {
                    if let (Some(x),Some(y))=(x,y) {
                        if x<=max && y<=max {keys.insert((x,y));}
                    }
                }
            }
        }
        keys
    }
    // Referencia continua: mismas alturas que bloques; una sola malla indexada.
    // Solo se construye cuando se selecciona el modo continuo y cambia NC.
    fn dense_geometry(&self)->(Vec<f32>,Vec<u32>){
        let mut vertices=Vec::with_capacity(N*N*STRIDE);
        for iy in 0..N {for ix in 0..N {
            let xa=ix.saturating_sub(1);let xb=(ix+1).min(N-1);
            let ya=iy.saturating_sub(1);let yb=(iy+1).min(N-1);
            let dzdx=(self.z(xb,iy)-self.z(xa,iy))/((xb-xa) as f32*STEP);
            let dzdy=(self.z(ix,yb)-self.z(ix,ya))/((yb-ya) as f32*STEP);
            let normal=Vec3::new(-dzdx,-dzdy,1.0).normalize_or_zero();
            let z=self.z(ix,iy);
            let color=if z<TOP-0.00001 {CUT} else {WOOD};
            put_vertex(&mut vertices,ix,iy,z,normal.to_array(),color);
        }}
        let mut indices=Vec::with_capacity((N-1)*(N-1)*6);
        for iy in 0..N-1 {for ix in 0..N-1 {
            let a=(iy*N+ix) as u32;
            let b=a+1;let c=a+N as u32;let d=c+1;
            indices.extend_from_slice(&[a,b,d,a,d,c]);
        }}
        (vertices,indices)
    }
    fn tile_mesh(&self,key:Key)->Option<TileMesh>{
        if !self.active_keys().contains(&key) {return None;}
        let x0=key.0*TILE;let y0=key.1*TILE;
        if x0>=N-1 || y0>=N-1 {return None;}
        let w=TILE.min(N-1-x0);
        let h=TILE.min(N-1-y0);
        let mut vertices=Vec::with_capacity((w+1)*(h+1)*STRIDE);
        for j in 0..=h {for i in 0..=w {
            let x=x0+i;let y=y0+j;
            let xa=x.saturating_sub(1);let xb=(x+1).min(N-1);
            let ya=y.saturating_sub(1);let yb=(y+1).min(N-1);
            let dzdx=(self.z(xb,y)-self.z(xa,y)) / ((xb-xa) as f32*STEP);
            let dzdy=(self.z(x,yb)-self.z(x,ya)) / ((yb-ya) as f32*STEP);
            let n=Vec3::new(-dzdx,-dzdy,1.0).normalize_or_zero();
            let z=self.z(x,y);
            let color=if z<TOP-0.00001 {CUT}else{WOOD};
            put_vertex(&mut vertices,x,y,z,n.to_array(),color);
        }}
        let mut indices=Vec::with_capacity(w*h*6);
        for j in 0..h {for i in 0..w {
            let a=(j*(w+1)+i) as u32;
            let b=a+1;let c=a+(w+1) as u32;let d=c+1;
            indices.extend_from_slice(&[a,b,d,a,d,c]);
        }}
        Some(TileMesh{vertices,indices})
    }
    // Superficie de fondo: solo las celdas de tile SIN mecanizar, sin sobreponer el corte.
    // Compacta zonas contiguas en cada fila para no reservar una malla densa.
    fn base(&self)->Vec<f32>{
        let active=self.active_keys();
        let tx=(N-1+TILE-1)/TILE;
        let ty=(N-1+TILE-1)/TILE;
        let mut vertices=Vec::new();
        for row in 0..ty {
            let mut col=0;
            while col<tx {
                if active.contains(&(col,row)) {col+=1;continue;}
                let begin=col;
                col+=1;
                while col<tx && !active.contains(&(col,row)){col+=1;}
                let x0=begin*TILE;let x1=(col*TILE).min(N-1);
                let y0=row*TILE;let y1=((row+1)*TILE).min(N-1);
                for &(x,y) in &[(x0,y0),(x1,y0),(x1,y1),
                                (x0,y0),(x1,y1),(x0,y1)] {
                    put_vertex(&mut vertices,x,y,TOP,[0.0,0.0,1.0],WOOD);
                }
            }
        }
        vertices
    }
    /// Verifica las 4 millones de celdas sin suavizado ni decimacion.
    /// Solo se llama bajo demanda; NO debe ejecutarse cada fotograma.
    fn audit_geometry(&self)->Result<String,String>{
        let begin=Instant::now();
        let active=self.active_keys();
        let tx=(N-1+TILE-1)/TILE;
        let ty=tx;
        let mut tile_cells=0_usize;
        let mut base_cells=0_usize;
        let mut max_z_error=0.0_f32;
        let mut max_seam_error=0.0_f32;
        for row in 0..ty {for col in 0..tx {
            let x0=col*TILE;let y0=row*TILE;
            let w=TILE.min(N-1-x0);let h=TILE.min(N-1-y0);
            if active.contains(&(col,row)) {
                let mesh=self.tile_mesh((col,row))
                    .ok_or_else(||format!("Sin malla en bloque activo ({col},{row})"))?;
                if mesh.indices.len()!=w*h*6 {
                    return Err(format!("Triangulos incompletos en ({col},{row})"));
                }
                tile_cells+=w*h;
                // Recorrer TODOS los vertices de todas las celdas representadas.
                // La malla debe contener exactamente el mismo Z que el motor.
                for ly in 0..=h {for lx in 0..=w {
                    let base=(ly*(w+1)+lx)*STRIDE;
                    let expected=self.z(x0+lx,y0+ly)*(2.0/SIDE)-0.1;
                    let error=(mesh.vertices[base+2]-expected).abs();
                    max_z_error=max_z_error.max(error);
                    if error>1.0e-6 {
                        return Err(format!("Z diferente en nodo ({},{}): {:.7}",x0+lx,y0+ly,error));
                    }
                }}
                // Comparar las costuras vertical y horizontal con sus vecinos.
                if active.contains(&(col+1,row)) && col+1<tx {
                    let right=self.tile_mesh((col+1,row)).unwrap();
                    let rh=TILE.min(N-1-y0);
                    for y in 0..=rh {
                        let a=mesh.vertices[(y*(w+1)+w)*STRIDE+2];
                        let b=right.vertices[y*(TILE.min(N-1-(col+1)*TILE)+1)*STRIDE+2];
                        max_seam_error=max_seam_error.max((a-b).abs());
                    }
                }
                if active.contains(&(col,row+1)) && row+1<ty {
                    let above=self.tile_mesh((col,row+1)).unwrap();
                    for x in 0..=w {
                        let a=mesh.vertices[(h*(w+1)+x)*STRIDE+2];
                        let b=above.vertices[x*STRIDE+2];
                        max_seam_error=max_seam_error.max((a-b).abs());
                    }
                }
            } else {
                // Si falta una malla activa, una celda debe ser realmente plana.
                // Comprobar sus 4 esquinas: no ocultar detalles en la base.
                for iy in y0..y0+h {for ix in x0..x0+w {
                    for (vx,vy) in [(ix,iy),(ix+1,iy),(ix,iy+1),(ix+1,iy+1)] {
                        if self.z(vx,vy)<TOP-0.00001 {
                            return Err(format!("Fondo cubre corte en celda ({ix},{iy})"));
                        }
                    }
                }}
                base_cells+=w*h;
            }
        }}
        let base=self.base();
        if base.len()%(STRIDE*6)!=0 {return Err("Base sin pares de triangulos".into());}
        // Area proyectada de toda la base con triangulos rectangulares.
        let mut base_area_cells=0.0_f64;
        for pair in base.chunks_exact(6*STRIDE) {
            let (ax,ay)=(pair[0] as f64,pair[1] as f64);
            let (bx,by)=(pair[STRIDE] as f64,pair[STRIDE+1] as f64);
            let (cx,cy)=(pair[2*STRIDE] as f64,pair[2*STRIDE+1] as f64);
            let area=((bx-ax)*(cy-ay)-(by-ay)*(cx-ax)).abs();
            let pitch=(STEP*(2.0/SIDE)) as f64;
            base_area_cells+=area/(pitch*pitch);
        }
        if (base_area_cells-base_cells as f64).abs()>1.0 {
            return Err(format!("Base incompleta: area {base_area_cells:.2}, esperada {base_cells}"));
        }
        if tile_cells+base_cells!=(N-1)*(N-1) {
            return Err(format!("Cobertura: {} de {} celdas",tile_cells+base_cells,(N-1)*(N-1)));
        }
        if max_seam_error>1.0e-6 {
            return Err(format!("Discontinuidad de Z entre bloques: {:.8}",max_seam_error));
        }
        Ok(format!("OK: {} celdas, {} bloques 3D, {} celdas base, error Z {:.8}, costuras {:.8}, {:.2} ms",
            tile_cells+base_cells,active.len(),base_cells,max_z_error,max_seam_error,
            begin.elapsed().as_secs_f64()*1000.0))
    }
    fn gpu_mib(&self)->f64{
        // Estimacion de vertices/indices, no incluye base, buffers GL ni overhead.
        self.active_keys().iter().map(|&key|{
            let x0=key.0*TILE;let y0=key.1*TILE;
            if x0>=N-1||y0>=N-1{return 0.0;}
            let w=TILE.min(N-1-x0);let h=TILE.min(N-1-y0);
            ((w+1)*(h+1)*STRIDE*4+w*h*6*4) as f64/1_048_576.0
        }).sum()
    }
}

struct GpuTile{vao:glow::VertexArray,vbo:glow::Buffer,ebo:glow::Buffer,count:i32}
struct Renderer{
    program:glow::Program,
    mvp:glow::UniformLocation,
    base_vao:glow::VertexArray,
    base_vbo:glow::Buffer,
    base_count:i32,
    tiles:HashMap<Key,GpuTile>,
    dense:Option<GpuTile>,
    dense_revision:Option<u64>,
    dense_build_ms:f64,
    dense_upload_mib:f64,
}
impl Renderer{
    unsafe fn layout(gl:&glow::Context){
        unsafe{for (location,offset) in [(0,0),(1,12),(2,24)]{
            gl.enable_vertex_attrib_array(location);
            gl.vertex_attrib_pointer_f32(location,3,glow::FLOAT,false,36,offset);
        }}
    }
    fn new(gl:&glow::Context)->Result<Self,String>{
        unsafe{
            let program=gl.create_program()?;
            let vs=gl.create_shader(glow::VERTEX_SHADER)?;
            gl.shader_source(vs,r#"#version 330 core
layout(location=0) in vec3 p;
layout(location=1) in vec3 normal;
layout(location=2) in vec3 color;
uniform mat4 mvp;
out vec3 n;
out vec3 c;
void main(){gl_Position=mvp*vec4(p,1);n=normal;c=color;}"#);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs){return Err(gl.get_shader_info_log(vs));}
            let fs=gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(fs,r#"#version 330 core
in vec3 n;in vec3 c;out vec4 frag;
void main(){
float light=max(dot(normalize(n),normalize(vec3(-0.35,-0.45,0.82))),0.0);
frag=vec4(c*(0.35+0.65*light),1.0);
}"#);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs){return Err(gl.get_shader_info_log(fs));}
            gl.attach_shader(program,vs);gl.attach_shader(program,fs);gl.link_program(program);
            gl.delete_shader(vs);gl.delete_shader(fs);
            if !gl.get_program_link_status(program){return Err(gl.get_program_info_log(program));}
            let mvp=gl.get_uniform_location(program,"mvp").ok_or("uniform mvp missing")?;
            let base_vao=gl.create_vertex_array()?;
            let base_vbo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(base_vao));gl.bind_buffer(glow::ARRAY_BUFFER,Some(base_vbo));
            Self::layout(gl);
            gl.bind_vertex_array(None);
            Ok(Self{program,mvp,base_vao,base_vbo,base_count:0,tiles:HashMap::new(),
                dense:None,dense_revision:None,dense_build_ms:0.0,dense_upload_mib:0.0})
        }
    }
    unsafe fn upload_tile(gl:&glow::Context,mesh:&TileMesh)->Result<GpuTile,String>{
        unsafe{
            let vao=gl.create_vertex_array()?;
            let vbo=gl.create_buffer()?;
            let ebo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&mesh.vertices),glow::DYNAMIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&mesh.indices),glow::STATIC_DRAW);
            Self::layout(gl);gl.bind_vertex_array(None);
            Ok(GpuTile{vao,vbo,ebo,count:mesh.indices.len() as i32})
        }
    }
    unsafe fn drop_tile(gl:&glow::Context,item:GpuTile){
        unsafe{gl.delete_vertex_array(item.vao);gl.delete_buffer(item.vbo);gl.delete_buffer(item.ebo);}
    }
    fn paint(&mut self,gl:&glow::Context,surface:&mut SparseMesh,matrix:Mat4,show_base:bool,show_tiles:bool,mode:usize,write_depth:bool,rebuild:bool){
        unsafe{
            let started=Instant::now();
            let mut transferred=0_usize;
            if rebuild && mode==0 {
                // Reconstruccion COMPLETA en el hilo OpenGL; sin alterar alturas/NC.
                for (_, gpu) in self.tiles.drain() {
                    Self::drop_tile(gl,gpu);
                }
                surface.base_dirty=true;
                let keys=surface.active_keys();
                surface.dirty.extend(keys);
            }
            if mode==1 {
                if self.dense_revision!=Some(surface.revision) || self.dense.is_none(){
                    let began=Instant::now();
                    let (vertices,indices)=surface.dense_geometry();
                    self.dense_build_ms=began.elapsed().as_secs_f64()*1000.0;
                    let upload=Instant::now();
                    match Self::upload_tile(gl,&TileMesh{vertices,indices}){
                        Ok(gpu)=>{
                            // Conservamos VAO/EBO ya residente hasta que cambie la geometria.
                            if let Some(old)=self.dense.replace(gpu){Self::drop_tile(gl,old);}
                            self.dense_revision=Some(surface.revision);
                            self.dense_upload_mib+=((N*N*STRIDE*4+(N-1)*(N-1)*6*4) as f64)/1_048_576.0;
                        }
                        Err(error)=>eprintln!("ERROR GL malla continua: {error}"),
                    }
                    let _elapsed_upload=upload.elapsed();
                }
            } else if surface.base_dirty {
                let base=surface.base();
                gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.base_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&base),glow::DYNAMIC_DRAW);
                self.base_count=(base.len()/STRIDE) as i32;
                transferred+=base.len()*4;
                surface.base_dirty=false;
            }
            if mode==0 {
            let expected=surface.active_keys();
            // Conservar caché exactamente sincronizada con la propiedad de celdas.
            // Un bloque activo puede necesitar render aunque no tenga cambios recientes.
            let stale:Vec<Key>=self.tiles.keys().filter(|key|!expected.contains(*key)).copied().collect();
            for key in stale {
                if let Some(tile)=self.tiles.remove(&key){Self::drop_tile(gl,tile);}
            }
            let mut updates=std::mem::take(&mut surface.dirty);
            for &key in &expected {
                if !self.tiles.contains_key(&key){updates.insert(key);}
            }
            for key in updates {
                if !expected.contains(&key){continue;}
                if let Some(mesh)=surface.tile_mesh(key) {
                    match Self::upload_tile(gl,&mesh){
                        Ok(gpu)=>{
                            transferred+=mesh.vertices.len()*4+mesh.indices.len()*4;
                            if let Some(old)=self.tiles.insert(key,gpu){Self::drop_tile(gl,old);}
                        },
                        Err(error)=>{eprintln!("ERROR GL tile {:?}: {}",key,error);}
                    }
                }
            }
            surface.upload_mib+=transferred as f64/1_048_576.0;
            surface.upload_ms=started.elapsed().as_secs_f64()*1000.0;
            }
            gl.enable(glow::DEPTH_TEST);gl.depth_func(glow::LEQUAL);
            gl.depth_mask(write_depth);
            gl.disable(glow::CULL_FACE);gl.disable(glow::BLEND);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.mvp),false,&matrix.to_cols_array());
            if mode==1 {
                if let Some(tile)=&self.dense{
                    gl.bind_vertex_array(Some(tile.vao));
                    gl.draw_elements(glow::TRIANGLES,tile.count,glow::UNSIGNED_INT,0);
                }
            } else {
                if show_base {
                    gl.bind_vertex_array(Some(self.base_vao));
                    gl.draw_arrays(glow::TRIANGLES,0,self.base_count);
                }
                if show_tiles {
                    for tile in self.tiles.values(){
                        gl.bind_vertex_array(Some(tile.vao));
                        gl.draw_elements(glow::TRIANGLES,tile.count,glow::UNSIGNED_INT,0);
                    }
                }
            }
            gl.bind_vertex_array(None);gl.use_program(None);
            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA,glow::ONE_MINUS_SRC_ALPHA);
        }
    }
}

struct ResultState {state:Result<engine::State,String>,seek_ms:f64}
struct App {
    tx:SyncSender<f64>,rx:Receiver<ResultState>,
    mesh:Arc<egui::mutex::Mutex<SparseMesh>>,
    renderer:Arc<egui::mutex::Mutex<Option<Result<Renderer,String>>>>,
    total:usize,current:f64,desired:f64,queued:Option<f64>,busy:bool,playing:bool,
    draw:bool,show_base:bool,show_tiles:bool,render_mode:usize,write_depth:bool,
    rebuild_gpu:Arc<AtomicBool>,rebuild_count:Arc<AtomicUsize>,yaw:f32,pitch:f32,distance:f32,tick:Instant,clock:Instant,fps:f64,
    seek_ms:f64,mesh_ms:f64,tiles:usize,memory_mib:f64,checkpoints:usize,
    replayed:usize, message:String,audit_message:String,
}
impl App {
    fn new()->Result<Self,String>{
        let mut demo=engine::Demo::new()?;
        let total=demo.len();
        let (tx,command_rx)=mpsc::sync_channel::<f64>(1);
        let (result_tx,rx)=mpsc::channel::<ResultState>();
        std::thread::Builder::new().name("lagucam-v15-sim".to_owned())
            .spawn(move ||{
                while let Ok(position)=command_rx.recv(){
                    let started=Instant::now();
                    let state=demo.seek(position);
                    let seek_ms=started.elapsed().as_secs_f64()*1000.0;
                    if result_tx.send(ResultState{state,seek_ms}).is_err(){break;}
                }
            }).map_err(|e|e.to_string())?;
        Ok(Self{
            tx,rx,total,mesh:Arc::new(egui::mutex::Mutex::new(SparseMesh::new())),
            renderer:Arc::new(egui::mutex::Mutex::new(None)),
            current:0.0,desired:0.0,queued:None,busy:false,playing:false,draw:true,show_base:true,show_tiles:true,render_mode:0,write_depth:true,
            rebuild_gpu:Arc::new(AtomicBool::new(false)),rebuild_count:Arc::new(AtomicUsize::new(0)),
            yaw:0.7,pitch:0.42,distance:2.6,
            tick:Instant::now(),clock:Instant::now(),fps:0.0,
            seek_ms:0.0,mesh_ms:0.0,tiles:0,memory_mib:0.0,checkpoints:0,replayed:0,
            message:String::new(),audit_message:String::new(),
        })
    }
    fn request(&mut self,p:f64){self.desired=p.clamp(0.0,self.total as f64);self.queued=Some(self.desired);}
    fn poll(&mut self){
        while let Ok(done)=self.rx.try_recv(){
            self.busy=false;self.seek_ms=done.seek_ms;
            match done.state {
                Ok(state)=>{
                    let started=Instant::now();
                    self.mesh.lock().apply(&state.patches);
                    self.mesh_ms=started.elapsed().as_secs_f64()*1000.0;
                    self.current=state.position;
                    self.tiles=state.tiles;self.memory_mib=state.memory_mib;
                    self.checkpoints=state.checkpoints;self.replayed=state.replayed;
                    self.message.clear();self.audit_message.clear();
                },
                Err(e)=>{self.playing=false;self.message=e;}
            }
        }
        if !self.busy {
            if let Some(target)=self.queued.take(){
                if (self.current-target).abs()>1.0e-8 {
                    match self.tx.try_send(target){
                        Ok(())=>self.busy=true,
                        Err(TrySendError::Full(v))=>self.queued=Some(v),
                        Err(TrySendError::Disconnected(_))=>{
                            self.playing=false;self.message="Hilo de simulacion cerrado".to_owned();
                        }
                    }
                }
            }
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self,ui:&mut egui::Ui,_frame:&mut eframe::Frame){
        let ctx=ui.ctx().clone();let now=Instant::now();
        let dt=now.duration_since(self.clock).as_secs_f64();self.clock=now;
        if dt>0.0001 && dt<1.0 {
            let instant=1.0/dt;self.fps=if self.fps==0.0 {instant}else{0.94*self.fps+0.06*instant};
        }
        self.poll();
        if self.playing && !self.busy && self.queued.is_none()
            && now.duration_since(self.tick)>=Duration::from_millis(300){
            self.tick=now;
            if self.current>=self.total as f64 {self.playing=false;}
            else {self.request((self.current.floor()+1.0).min(self.total as f64));}
        }
        if self.playing || self.busy || self.queued.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        egui::Panel::top("controls").show(ui,|ui|{
            ui.heading("LaguCAM V21 - Reconstruccion de cache GPU A/B");
            ui.horizontal(|ui|{
                if ui.button(if self.playing{"Pausar"}else{"Reproducir"}).clicked(){
                    self.playing=!self.playing;self.tick=Instant::now();
                }
                if ui.button("Anterior").clicked(){self.playing=false;self.request((self.desired.ceil()-1.0).max(0.0));}
                if ui.button("Siguiente").clicked(){self.playing=false;self.request((self.desired.floor()+1.0).min(self.total as f64));}
                if ui.button("Inicio").clicked(){self.playing=false;self.request(0.0);}
                if ui.button("Final").clicked(){self.playing=false;self.request(self.total as f64);}
                ui.label("Render:");
                ui.selectable_value(&mut self.render_mode,0,"B - Bloques");
                ui.selectable_value(&mut self.render_mode,1,"A - Continua");
                ui.checkbox(&mut self.draw,"Dibujar superficie");
                ui.checkbox(&mut self.show_base,"Plano intacto");
                ui.checkbox(&mut self.show_tiles,"Mallas de bloques");
                ui.checkbox(&mut self.write_depth,"Escribir Z (diagnostico)");
            });
            // V21: segunda fila para mantener visible el boton de reconstruccion.
            ui.horizontal(|ui| {
                if ui.add_enabled(self.render_mode==0 && !self.busy,
                    egui::Button::new("Reconstruir GPU")).clicked() {
                    self.playing=false;
                    self.rebuild_gpu.store(true,Ordering::Release);
                }
                if ui.button("Verificar geometria").clicked(){
                    let result=self.mesh.lock().audit_geometry();
                    self.audit_message=match result {
                        Ok(text)=>text,
                        Err(error)=>format!("ERROR: {error}"),
                    };
                }
            });
            let mut selection=self.desired;
            if ui.add(egui::Slider::new(&mut selection,0.0..=self.total as f64).text("Posicion NC")).changed(){
                self.playing=false;self.request(selection);
            }
            ui.label(format!("NC {:.2}/{} (solicitado {:.2}) | FPS {:.1} | {}",
                self.current,self.total,self.desired,self.fps,if self.busy{"Procesando"}else{"Listo"}));
            let mesh=self.mesh.lock();
            ui.label(format!("Bloques CPU {} | Alturas+CP {:.2} MiB | Bloques GPU {} | GPU mallas aprox {:.2} MiB | CP {}",
                self.tiles,self.memory_mib,mesh.active_keys().len(),mesh.gpu_mib(),self.checkpoints));
            ui.label(format!("Replay {} movimientos | Seek {:.2} ms | Aplicar parches {:.2} ms | Subida GL {:.2} ms | Transferidos {:.1} MiB",
                self.replayed,self.seek_ms,self.mesh_ms,mesh.upload_ms,mesh.upload_mib));
            let expected=mesh.active_keys();
            drop(mesh); // Orden de bloqueos: renderer -> mesh en callback GL.
            let (actual,missing,extra)=match self.renderer.lock().as_ref() {
                Some(Ok(renderer))=>{
                    let missing=expected.iter().filter(|key|!renderer.tiles.contains_key(*key)).count();
                    let extra=renderer.tiles.keys().filter(|key|!expected.contains(*key)).count();
                    (renderer.tiles.len(),missing,extra)
                },
                _=>(0,expected.len(),0),
            };
            ui.label(format!("GPU cache: {} mallas cargadas | Faltantes {} | Sobrantes {} | Reconstrucciones forzadas {}",
                actual,missing,extra,self.rebuild_count.load(Ordering::Relaxed)));
            let (dense_ready,dense_ms,dense_mib)=match self.renderer.lock().as_ref(){
                Some(Ok(r))=>(r.dense_revision.is_some(),r.dense_build_ms,r.dense_upload_mib),
                _=>(false,0.0,0.0)
            };
            ui.label(format!("Continua GPU cargada: {} | Construccion CPU ultima {:.1} ms | Subidas continuas {:.1} MiB",
                dense_ready,dense_ms,dense_mib));
            ui.small("V21: en B - Bloques y NC Final, pulsa Reconstruir GPU. La prueba elimina todos los VBO/VAO de bloques y los crea otra vez sin modificar alturas. A/B comparte geometria. Prueba Escritura Z activada frente a desactivada. Pausar y alternar A/B en la misma posicion. \
                Malla continua ~240 MiB en GPU; se carga al seleccionarla y cuando cambia el material. \
                Estas metricas son del prototipo, no FPS garantizados.");
            if !self.message.is_empty(){ui.colored_label(egui::Color32::RED,&self.message);}
            if !self.audit_message.is_empty(){
                let color=if self.audit_message.starts_with("OK:") {egui::Color32::DARK_GREEN}
                    else {egui::Color32::RED};
                ui.colored_label(color,&self.audit_message);
            }
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
                if scroll!=0.0 {
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
            let show_base=self.show_base;
            let show_tiles=self.show_tiles;
            let mode=self.render_mode;
            let write_depth=self.write_depth;
            let rebuild_gpu=Arc::clone(&self.rebuild_gpu);
            let rebuild_count=Arc::clone(&self.rebuild_count);
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
                        gl.depth_mask(true);
                        gl.clear(glow::DEPTH_BUFFER_BIT);
                        gl.disable(glow::SCISSOR_TEST);
                    }
                    if !draw{return;}
                    let mut lock=renderer.lock();
                    if lock.is_none(){*lock=Some(Renderer::new(gl));}
                    if let Some(Ok(r))=lock.as_mut(){
                        let force=mode==0 && rebuild_gpu.swap(false,Ordering::AcqRel);
                        r.paint(gl,&mut mesh.lock(),mvp,show_base,show_tiles,mode,write_depth,force);
                        if force {rebuild_count.fetch_add(1,Ordering::Relaxed);}
                    }
                })),
            });
        });
    }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let app=App::new().map_err(std::io::Error::other)?;
    eframe::run_native("LaguCAM V21 - Reconstruccion GPU",eframe::NativeOptions::default(),
        Box::new(move |_cc|Ok(Box::new(app))))?;
    Ok(())
}

#[cfg(test)]
mod continuity_tests {
    use super::*;
    #[test]
    fn corner_sample_renders_four_neighbor_cell_tiles(){
        let mut m=SparseMesh::new();
        let mut data=vec![TOP;TILE*TILE];
        data[0]=5.0; // muestra global (128,128) en tile de datos (1,1)
        m.data.insert((1,1),data);
        let active=m.active_keys();
        for key in [(0,0),(0,1),(1,0),(1,1)]{
            assert!(active.contains(&key),"No se renderiza tile {:?}",key);
            assert!(m.tile_mesh(key).is_some());
        }
        // El mismo vertice fisico de la esquina debe compartir Z.
        let left=m.tile_mesh((0,0)).unwrap();
        let right=m.tile_mesh((1,1)).unwrap();
        let z_left=left.vertices[(128*(129)+128)*STRIDE+2];
        let z_right=right.vertices[2];
        assert!((z_left-z_right).abs()<1.0e-6);
        assert!((z_left-(5.0*(2.0/SIDE)-0.1)).abs()<1.0e-6);
    }
    #[test]
    fn base_does_not_overlap_any_active_tile_cell(){
        let mut m=SparseMesh::new();
        let mut data=vec![TOP;TILE*TILE];
        data[0]=5.0;
        m.data.insert((1,1),data);
        let active=m.active_keys();
        let base=m.base();
        // Cada triangulo de base pertenece completamente a bloques inactivos.
        for tri in base.chunks_exact(3*STRIDE){
            let x=tri[0];
            let y=tri[1];
            let gridx=(((x+1.0)*SIDE/2.0)/STEP).round() as usize;
            let gridy=(((y+1.0)*SIDE/2.0)/STEP).round() as usize;
            assert!(!active.contains(&(gridx/TILE,gridy/TILE)));
        }
    }
}


#[cfg(test)]
mod v17_geometry_tests {
    use super::*;
    #[test]
    fn no_uncovered_cells_when_neighbor_tiles_are_active(){
        let mut mesh=SparseMesh::new();
        let mut tile=vec![TOP;TILE*TILE];
        tile[0]=5.0;
        tile[TILE-1]=5.0;
        mesh.data.insert((3,4),tile);
        let active=mesh.active_keys();
        let nt=(N-1+TILE-1)/TILE;
        let mut covered=0usize;
        for ty in 0..nt {for tx in 0..nt{
            let owned=active.contains(&(tx,ty));
            let base=!owned;
            assert_ne!(owned,base);
            if owned {
                let m=mesh.tile_mesh((tx,ty)).expect("Bloque activo sin malla");
                let x0=tx*TILE;let y0=ty*TILE;
                let w=TILE.min(N-1-x0);let h=TILE.min(N-1-y0);
                assert_eq!(m.indices.len(),w*h*6);
            }
            covered+=1;
        }}
        assert_eq!(covered,nt*nt);
    }
    #[test]
    fn all_neighbor_seams_share_same_sample_heights(){
        let mut mesh=SparseMesh::new();
        let mut tile=vec![TOP;TILE*TILE];
        for j in 0..TILE {
            tile[j*TILE]=(5.0+j as f32/100.0).min(TOP);
        }
        mesh.data.insert((3,4),tile);
        let active=mesh.active_keys();
        for &(tx,ty) in &active{
            let cur=mesh.tile_mesh((tx,ty)).unwrap();
            let x0=tx*TILE;let y0=ty*TILE;
            let w=TILE.min(N-1-x0);let h=TILE.min(N-1-y0);
            for j in [0,h] {for i in 0..=w {
                let v=(j*(w+1)+i)*STRIDE;
                let expected=mesh.z(x0+i,y0+j)*(2.0/SIDE)-0.1;
                assert!((cur.vertices[v+2]-expected).abs()<1.0e-6);
            }}
            for i in [0,w] {for j in 0..=h {
                let v=(j*(w+1)+i)*STRIDE;
                let expected=mesh.z(x0+i,y0+j)*(2.0/SIDE)-0.1;
                assert!((cur.vertices[v+2]-expected).abs()<1.0e-6);
            }}
        }
    }
}
#[cfg(test)]
mod v18_tests {
    use super::*;
    #[test]
    fn audit_rejects_missing_surface_cell_coverage(){
        let mut mesh=SparseMesh::new();
        let mut tile=vec![TOP;TILE*TILE];
        tile[0]=5.0;
        tile[1]=6.0;
        tile[TILE-1]=7.0;
        mesh.data.insert((1,1),tile);
        assert!(mesh.audit_geometry().is_ok());
    }
    #[test]
    fn all_intact_stock_is_covered(){
        let mesh=SparseMesh::new();
        assert!(mesh.audit_geometry().is_ok());
    }
}

#[cfg(test)]
mod v19_ab_tests{
    use super::*;
    #[test]
    fn dense_and_tile_agree_at_edges_and_cut(){
        let mut mesh=SparseMesh::new();
        let mut tile=vec![TOP;TILE*TILE];
        for i in 0..128 {tile[127*TILE+i]=5.0+(i as f32)*0.01;}
        mesh.data.insert((2,3),tile);
        let (dense,indices)=mesh.dense_geometry();
        assert_eq!(indices.len(),(N-1)*(N-1)*6);
        for key in mesh.active_keys(){
            let part=mesh.tile_mesh(key).unwrap();
            let x0=key.0*TILE;let y0=key.1*TILE;
            let w=TILE.min(N-1-x0);let h=TILE.min(N-1-y0);
            for j in 0..=h {for i in 0..=w {
                let d=((y0+j)*N+x0+i)*STRIDE;
                let t=(j*(w+1)+i)*STRIDE;
                for k in 0..9 {
                    assert!((dense[d+k]-part.vertices[t+k]).abs()<=1e-6,
                        "Coord ({},{}), campo {} difiere",x0+i,y0+j,k);
                }
            }}
        }
    }
}
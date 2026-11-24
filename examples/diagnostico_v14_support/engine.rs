//! V12: NC interpretado, 2.5D disperso, checkpoints COW y seek arbitrario.
//! Prototipo aislado: NO modifica el motor ni interpreta codigos NC adicionales.
use cam_libre_visor::material_sim::{
    segments_from_trace, CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox,
};
use cam_libre_visor::nc_contract::{NcDistanceMode,NcInitialState,NcSpindleMode,NcUnits};
use cam_libre_visor::nc_interpreter::interpret_program;
use std::collections::{HashMap,HashSet};
use std::sync::{Arc,atomic::{AtomicBool,AtomicUsize}};
use std::time::Instant;

const SIDE:f64=100.0;
const TOP:f64=10.0;
const DIAM:f64=2.0;
const TILE:usize=128;
const SNAP_EVERY:usize=25;
const MAX_SNAPS:usize=4;
const MAX_STORED_MIB:f64=64.0;
const TOL:f64=1.0e-5;

type Key=(usize,usize);
type Tiles=HashMap<Key,Arc<[f32]>>;

fn gcode(passes:usize)->String {
    let mut text=String::from("G21 G90 G94\nG0 X20 Y20 Z12\nS12000 M3\nF600\n");
    for i in 0..passes {
        let y=20.0+i as f64*0.8;
        let (a,b)=if i%2==0 {(20.0,45.0)} else {(45.0,20.0)};
        if i>0 {text.push_str("G0 Z12\n");}
        if i>0 {text.push_str(&format!("G0 X{a:.3} Y{y:.3}\n"));}
        text.push_str(&format!("G1 Z5\nG1 X{b:.3} Y{y:.3}\n"));
    }
    text.push_str("G0 Z12\nM5\nG0 X0 Y0 Z12\nM30\n");
    text
}
fn segments(n:usize)->Result<Vec<CutSegment>,String>{
    let initial=NcInitialState{
        position_work_mm:[0.0,0.0,12.0],
        units:NcUnits::Millimeters,
        distance_mode:NcDistanceMode::Absolute,
        feed_mm_min:None,
        spindle_rpm:None,
        spindle_mode:NcSpindleMode::Stopped,
        tool_number:None,
    };
    let trace=interpret_program(&gcode(n),initial).map_err(|e|e.to_string())?;
    segments_from_trace(&trace,Ok)
}
struct Surface {
    step:f64,
    nx:usize,
    tiles:Tiles,
}
impl Surface {
    fn new(step:f64)->Self {
        let nx=(SIDE/step).round() as usize+1;
        Self{step,nx,tiles:HashMap::new()}
    }
    fn height(&self,ix:usize,iy:usize)->f32 {
        self.tiles.get(&(ix/TILE,iy/TILE))
            .map(|a|a[(iy%TILE)*TILE+ix%TILE]).unwrap_or(TOP as f32)
    }
    fn stamp(&mut self,tip:[f64;3],shape:CutterShape) {
        if tip[2]>=TOP || tip[2]+12.0<=0.0 {return;}
        let r=DIAM*0.5;
        let to_index=|v:f64,ceil:bool|->usize{
            let n=(v/self.step).clamp(0.0,(self.nx-1) as f64);
            if ceil {n.ceil() as usize} else {n.floor() as usize}
        };
        let x0=to_index(tip[0]-r,false);let x1=to_index(tip[0]+r,true);
        let y0=to_index(tip[1]-r,false);let y1=to_index(tip[1]+r,true);
        for iy in y0..=y1 {
            let dy=iy as f64*self.step-tip[1];
            for ix in x0..=x1 {
                let dx=ix as f64*self.step-tip[0];
                let rad=dx*dx+dy*dy;
                if rad>r*r {continue;}
                let bottom=match shape{
                    CutterShape::Flat=>0.0,
                    CutterShape::Ball=>r-(r*r-rad).max(0.0).sqrt(),
                };
                let z=(tip[2]+bottom).clamp(0.0,TOP) as f32;
                if z>=TOP as f32 {continue;}
                let key=(ix/TILE,iy/TILE);
                let values=self.tiles.entry(key)
                    .or_insert_with(||Arc::from(vec![TOP as f32;TILE*TILE]));
                let index=(iy%TILE)*TILE+ix%TILE;
                if z<values[index] {
                    Arc::make_mut(values)[index]=z;
                }
            }
        }
    }
    fn apply(&mut self,s:&CutSegment,shape:CutterShape,fraction:f64)->Result<(),String>{
        if !(0.0..=1.0).contains(&fraction) {return Err("Fraccion invalida".into());}
        if !s.spindle_on {return Ok(());}
        let r=DIAM*0.5;
        let contact=(0..3).all(|axis|{
            let below=if axis==2 {0.0} else {r};
            let above=if axis==2 {12.0} else {r};
            let lo=s.from_model_mm[axis].min(s.to_model_mm[axis])-below;
            let hi=s.from_model_mm[axis].max(s.to_model_mm[axis])+above;
            hi>=0.0 && lo<=if axis==2 {TOP} else {SIDE}
        });
        if !contact {return Ok(());}
        let delta:[f64;3]=std::array::from_fn(|axis|s.to_model_mm[axis]-s.from_model_mm[axis]);
        let len=delta.iter().map(|v|v*v).sum::<f64>().sqrt();
        let n=((len/(self.step*0.5)).ceil() as usize).max(1);
        if n>8_000_000 {return Err("Barrido demasiado largo".into());}
        let last=((n as f64*fraction).floor() as usize).min(n);
        for j in 0..=last {
            let t=j as f64/n as f64;
            let tip=std::array::from_fn(|axis|s.from_model_mm[axis]+delta[axis]*t);
            self.stamp(tip,shape);
        }
        Ok(())
    }
    fn tiles_mib(&self)->f64 {self.tiles.len() as f64*(TILE*TILE*4) as f64/1048576.0}
}
struct Snapshot{completed:usize,tiles:Tiles}
struct Player {
    shape:CutterShape,
    surface:Surface,
    snapshots:Vec<Snapshot>,
    replayed:usize,
}
impl Player {
    fn new(step:f64,shape:CutterShape)->Self {
        Self{shape,surface:Surface::new(step),snapshots:Vec::new(),replayed:0}
    }
    fn make_snapshot(&mut self,completed:usize) {
        self.snapshots.push(Snapshot{completed,tiles:self.surface.tiles.clone()});
        while self.snapshots.len()>MAX_SNAPS || self.unique_mib()>MAX_STORED_MIB {
            self.snapshots.remove(0);
            if self.snapshots.is_empty() {break;}
        }
    }
    fn seek(&mut self,segments:&[CutSegment],completed:usize,partial:f64)->Result<(),String>{
        if completed>segments.len() || !(0.0..=1.0).contains(&partial) ||
            (completed==segments.len() && partial!=0.0) {return Err("Posicion NC invalida".into());}
        // Recuperar la instantanea <= destino; si fue expulsada, recomputar desde el bruto.
        let best=self.snapshots.iter().filter(|s|s.completed<=completed)
            .max_by_key(|s|s.completed);
        let start=if let Some(cp)=best {
            self.surface.tiles=cp.tiles.clone();
            cp.completed
        } else {
            self.surface.tiles.clear();
            0
        };
        self.replayed=completed-start;
        for i in start..completed {
            self.surface.apply(&segments[i],self.shape,1.0)?;
            if (i+1)%SNAP_EVERY==0 && !self.snapshots.iter().any(|cp|cp.completed==i+1){
                self.make_snapshot(i+1);
            }
        }
        if partial>0.0 {
            self.surface.apply(&segments[completed],self.shape,partial)?;
        }
        Ok(())
    }
    fn unique_mib(&self)->f64 {
        let mut addresses=HashSet::new();
        for data in self.surface.tiles.values() {
            addresses.insert(data.as_ptr() as usize);
        }
        for snap in &self.snapshots {
            for data in snap.tiles.values(){addresses.insert(data.as_ptr() as usize);}
        }
        addresses.len() as f64*(TILE*TILE*4) as f64/1048576.0
    }
}
fn same(a:&Surface,b:&Surface)->f64 {
    let mut maxerr=0.0_f64;
    let keys:HashSet<Key>=a.tiles.keys().chain(b.tiles.keys()).copied().collect();
    for key in keys {
        for pos in 0..(TILE*TILE) {
            let h1=a.tiles.get(&key).map(|v|v[pos]).unwrap_or(TOP as f32);
            let h2=b.tiles.get(&key).map(|v|v[pos]).unwrap_or(TOP as f32);
            maxerr=maxerr.max((h1-h2).abs() as f64);
        }
    }
    maxerr
}
fn compare_reference(segs:&[CutSegment],shape:CutterShape)->Result<f64,String>{
    let n=(SIDE/0.1) as usize+1;
    let mut reference=MaterialHeightfield::simulate_with_budget(
        StockBox{min_mm:[0.0;3],size_mm:[SIDE,SIDE,TOP]},
        CutterSpec{shape,diameter_mm:DIAM,cutting_length_mm:12.0},
        n,512,segs,&AtomicBool::new(false),&AtomicUsize::new(0)
    )?;
    let mut player=Player::new(0.1,shape);
    let mut worst=0.0_f64;
    for completed in [0,1,4,7,11,16,20,segs.len()] {
        let c=completed.min(segs.len());
        player.seek(segs,c,0.0)?;
        reference.seek(c as f64);
        for iy in 0..n {
            for ix in 0..n {
                worst=worst.max((player.surface.height(ix,iy) as f64-
                    reference.height_at(ix,iy)).abs());
            }
        }
    }
    if worst>TOL {return Err(format!("Diferencia frente a motor original: {worst:.7} mm"));}
    Ok(worst)
}
fn run(step:f64,shape:CutterShape,segs:&[CutSegment])->Result<(),String>{
    let started=Instant::now();
    let mut player=Player::new(step,shape);
    player.seek(segs,segs.len(),0.0)?;
    let tiles=player.surface.tiles.len();
    let final_state=Surface{step,nx:player.surface.nx,tiles:player.surface.tiles.clone()};
    let initial_snapshots=player.snapshots.len();
    let mem=player.unique_mib();
    // Saltos fuera de orden, incluyendo seek hacia atras hasta el bruto original.
    let checks=[segs.len()/2,0,segs.len()*3/4,segs.len()/4,segs.len(),
                segs.len()/3,segs.len()];
    let mut worst=0.0_f64;
    let mut max_replayed=0_usize;
    for &completed in &checks {
        player.seek(segs,completed,0.0)?;
        max_replayed=max_replayed.max(player.replayed);
        let mut cold=Player::new(step,shape);
        cold.seek(segs,completed,0.0)?;
        worst=worst.max(same(&player.surface,&cold.surface));
        if worst>TOL {return Err(format!("Diferencia tras salto {completed}: {worst:.8}"));}
    }
    player.seek(segs,segs.len(),0.0)?;
    worst=worst.max(same(&player.surface,&final_state));
    // Fraccion de movimiento G1: misma discretizacion y equivalencia por reconstruccion.
    let move_index=segs.iter().position(|s|s.spindle_on && !s.rapid)
        .ok_or("No hay G1 con husillo")?;
    player.seek(segs,move_index,0.5)?;
    let mut cold=Player::new(step,shape);
    cold.seek(segs,move_index,0.5)?;
    worst=worst.max(same(&player.surface,&cold.surface));
    if worst>TOL {return Err(format!("Diferencia fraccion G1: {worst:.8}"));}
    println!("{:>8} | {:>4.2} | {:>4} | {:>5} | {:>4} | {:>8.2} | {:>8.2} | {:>4} | {:>7.3} | {:>9.7} | OK",
        match shape{CutterShape::Flat=>"plana",CutterShape::Ball=>"esferica"},
        step,segs.len(),tiles,initial_snapshots,
        final_state.tiles.len() as f64*(TILE*TILE*4) as f64/1048576.0,
        mem,max_replayed,started.elapsed().as_secs_f64(),worst);
    Ok(())
}
fn main()->Result<(),String>{
    let segs=segments(60)?;
    println!("LAGUCAM V12 - NC real, checkpoints COW, seek/replay arbitrario");
    println!("{} movimientos G0/G1; 60 pasadas; 0.05 mm; 128x128 muestras/tile",segs.len());
    println!("Checkpoints cada {} movimientos, maximo {} o {:.0} MiB de alturas unicas (incluye superficie activa)",SNAP_EVERY,MAX_SNAPS,MAX_STORED_MIB);
    println!("Memoria UNICA estimada de matrices de alturas, no RAM total del proceso");
    println!("Fresa    | Paso | Movs | Tiles | CPs | Surf MiB | Mem MiB  | Replay | Tiempo s | Error mm  | Estado");
    for shape in [CutterShape::Flat,CutterShape::Ball] {
        run(0.05,shape,&segs)?;
    }
    let short=segments(6)?;
    for shape in [CutterShape::Flat,CutterShape::Ball] {
        let error=compare_reference(&short,shape)?;
        println!("Referencia motor actual 0.10 mm {} ({} movs): error {:.7} mm -> OK",
            match shape{CutterShape::Flat=>"plana",CutterShape::Ball=>"esferica"},short.len(),error);
    }
    println!("FIN: prototipo sin OpenGL; CP COW, no historial de modificaciones puntuales.");
    println!("NOTA: los checkpoints son limitados EN NUMERO, no por bytes; saltos antiguos recomputan.");
    Ok(())
}
// V14 API: devolver exclusivamente tiles con alturas realmente diferentes.
pub struct TilePatch {
    pub tx:usize,
    pub ty:usize,
    pub values:Option<Vec<f32>>,
}
pub struct Demo {
    segments:Vec<CutSegment>,
    player:Player,
}
pub struct State {
    pub position:f64,
    pub patches:Vec<TilePatch>,
    pub tiles:usize,
    pub memory_mib:f64,
    pub checkpoints:usize,
    pub replayed:usize,
}
impl Demo {
    pub fn new()->Result<Self,String>{
        let segments=segments(60)?;
        Ok(Self{segments,player:Player::new(0.05,CutterShape::Ball)})
    }
    pub fn len(&self)->usize{self.segments.len()}
    pub fn seek(&mut self,position:f64)->Result<State,String>{
        if !position.is_finite() || !(0.0..=self.len() as f64).contains(&position){
            return Err("Posicion NC invalida".into());
        }
        let before=self.player.surface.tiles.clone();
        let completed=position.floor() as usize;
        let fraction=if completed==self.len(){0.0}else{position-completed as f64};
        self.player.seek(&self.segments,completed,fraction)?;
        let after=&self.player.surface.tiles;
        let keys:HashSet<Key>=before.keys().chain(after.keys()).copied().collect();
        let mut patches=Vec::new();
        for (tx,ty) in keys{
            let old=before.get(&(tx,ty));
            let new=after.get(&(tx,ty));
            if let (Some(a),Some(b))=(old,new){
                if Arc::ptr_eq(a,b) || a.as_ref()==b.as_ref(){continue;}
            }else if old.is_none() && new.is_none(){continue;}
            patches.push(TilePatch{tx,ty,values:new.map(|a|a.as_ref().to_vec())});
        }
        patches.sort_unstable_by_key(|p|(p.ty,p.tx));
        Ok(State{
            position,patches,tiles:after.len(),
            memory_mib:self.player.unique_mib(),
            checkpoints:self.player.snapshots.len(),
            replayed:self.player.replayed,
        })
    }
}
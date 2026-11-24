//! LaguCAM V11: prueba aislada de NC real, heightfield disperso y rebobinado por segmento.
//! No modifica el codigo productivo. Limitacion: un solo bruto, una herramienta, 3 ejes, 2.5D.
use cam_libre_visor::material_sim::{
    segments_from_trace, CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox,
};
use cam_libre_visor::nc_contract::{NcDistanceMode, NcInitialState, NcSpindleMode, NcUnits};
use cam_libre_visor::nc_interpreter::interpret_program;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Instant;

const TILE: usize = 128;
const SIDE: f64 = 100.0;
const HEIGHT: f64 = 10.0;
const DIAM: f64 = 5.0;
const EPS: f64 = 0.000_01;
type Key = (usize, usize);
type Data = Box<[f32]>;

const NC: &str = "\
G21 G90 G94
G0 X10 Y10 Z12
S12000 M3
G1 F800 Z5
G1 X90 Y10
G0 Z12
G0 X90 Y30
G1 Z5
G1 X10 Y30
G0 Z12
M5
G0 X0 Y0 Z12
M30
";

struct Before { key: Key, previous: Option<Data> }
struct Checkpoint { before: Vec<Before>, bytes: usize }
struct Sparse {
    step: f64,
    nx: usize,
    tiles: HashMap<Key,Data>,
    history: Vec<Checkpoint>,
    changed: usize,
}
impl Sparse {
    fn new(step: f64) -> Result<Self,String> {
        if !(step.is_finite() && step > 0.0) {return Err("Paso no valido".into());}
        let n = SIDE / step;
        if (n - n.round()).abs()>1e-8 {return Err("El lado debe ser multiplo del paso".into());}
        Ok(Self {step,nx:n.round() as usize+1, tiles:HashMap::new(), history:Vec::new(),changed:0})
    }
    fn height(&self,ix:usize,iy:usize)->f32 {
        self.tiles.get(&(ix/TILE,iy/TILE))
            .map(|tile|tile[(iy%TILE)*TILE+(ix%TILE)])
            .unwrap_or(HEIGHT as f32)
    }
    fn lower(&mut self,ix:usize,iy:usize,z:f32) {
        if z>=HEIGHT as f32 {return;}
        let tile=self.tiles.entry((ix/TILE,iy/TILE))
            .or_insert_with(||vec![HEIGHT as f32;TILE*TILE].into_boxed_slice());
        let index=(iy%TILE)*TILE+(ix%TILE);
        if z<tile[index] {tile[index]=z;self.changed+=1;}
    }
    fn region(&self, seg: &CutSegment) -> (usize,usize,usize,usize) {
        let r=DIAM/2.0;
        let coord=|mm:f64,ceil:bool| {
            let p=(mm/self.step).clamp(0.0,(self.nx-1) as f64);
            if ceil {p.ceil() as usize} else {p.floor() as usize}
        };
        (coord(seg.from_model_mm[0].min(seg.to_model_mm[0])-r,false),
         coord(seg.from_model_mm[0].max(seg.to_model_mm[0])+r,true),
         coord(seg.from_model_mm[1].min(seg.to_model_mm[1])-r,false),
         coord(seg.from_model_mm[1].max(seg.to_model_mm[1])+r,true))
    }
    fn apply_tip(&mut self, tip:[f64;3], shape:CutterShape) {
        let radius=DIAM/2.0;
        // Replica los descartes del motor productivo (corte sobre el bruto).
        if tip[2]>=HEIGHT || tip[2]+12.0<=0.0 {return;}
        let pos=|mm:f64, ceil:bool| {
            let p=(mm/self.step).clamp(0.0,(self.nx-1) as f64);
            if ceil {p.ceil() as usize} else {p.floor() as usize}
        };
        let x0=pos(tip[0]-radius,false);
        let x1=pos(tip[0]+radius,true);
        let y0=pos(tip[1]-radius,false);
        let y1=pos(tip[1]+radius,true);
        for iy in y0..=y1 {
            let y=iy as f64*self.step;
            let dy=y-tip[1];
            for ix in x0..=x1 {
                let x=ix as f64*self.step;
                let dx=x-tip[0];
                let r2=dx*dx+dy*dy;
                if r2>radius*radius {continue;}
                let bottom=match shape {
                    CutterShape::Flat=>0.0,
                    CutterShape::Ball=>radius-(radius*radius-r2).max(0.0).sqrt(),
                };
                let z=(tip[2]+bottom).clamp(0.0,HEIGHT) as f32;
                self.lower(ix,iy,z);
            }
        }
    }
    fn segment_samples(&self,seg:&CutSegment)->usize {
        let length=seg.from_model_mm.iter().zip(seg.to_model_mm)
            .map(|(a,b)|(b-*a).powi(2)).sum::<f64>().sqrt();
        ((length/(self.step*0.5)).ceil() as usize).max(1)
    }
    // max_fraction controla el instante NC interpretado, NO crea un segmento nuevo.
    fn apply(&mut self,seg:&CutSegment,shape:CutterShape,max_fraction:f64)->Result<(),String> {
        if !(0.0..=1.0).contains(&max_fraction) {return Err("Fraccion invalida".into());}
        let mut checkpoint=Checkpoint{before:Vec::new(),bytes:0};
        let r=DIAM/2.0;
        let stock_contact = (0..3).all(|axis| {
            let below=if axis==2 {0.0} else {r};
            let above=if axis==2 {12.0} else {r};
            let min=seg.from_model_mm[axis].min(seg.to_model_mm[axis])-below;
            let max=seg.from_model_mm[axis].max(seg.to_model_mm[axis])+above;
            min<=(if axis==2 {HEIGHT} else {SIDE}) && max>=0.0
        });
        if seg.spindle_on && stock_contact {
            let (x0,x1,y0,y1)=self.region(seg);
            for ty in y0/TILE..=y1/TILE {
                for tx in x0/TILE..=x1/TILE {
                    let key=(tx,ty);
                    let previous=self.tiles.get(&key).cloned();
                    if previous.is_some() {checkpoint.bytes+=TILE*TILE*4;}
                    checkpoint.before.push(Before{key,previous});
                }
            }
            let total=self.segment_samples(seg);
            let limit=((total as f64)*max_fraction).floor() as usize;
            let delta:[f64;3]=std::array::from_fn(|i|seg.to_model_mm[i]-seg.from_model_mm[i]);
            for s in 0..=limit {
                let t=s as f64/total as f64;
                let tip=std::array::from_fn(|axis|seg.from_model_mm[axis]+delta[axis]*t);
                self.apply_tip(tip,shape);
            }
        }
        self.history.push(checkpoint);
        Ok(())
    }
    fn undo(&mut self)->Result<(),String> {
        let snapshot=self.history.pop().ok_or("Historial vacio")?;
        for previous in snapshot.before {
            match previous.previous {
                Some(tile)=>{self.tiles.insert(previous.key,tile);},
                None=>{self.tiles.remove(&previous.key);},
            }
        }
        Ok(())
    }
    fn mb(&self)->f64 {self.tiles.len() as f64*(TILE*TILE*4) as f64/1048576.0}
    fn history_mb(&self)->f64 {self.history.iter().map(|x|x.bytes).sum::<usize>() as f64/1048576.0}
}

fn program()->Result<Vec<CutSegment>,String>{
    let initial=NcInitialState{
        position_work_mm:[0.0,0.0,12.0],
        units:NcUnits::Millimeters,
        distance_mode:NcDistanceMode::Absolute,
        feed_mm_min:None,
        spindle_rpm:None,
        spindle_mode:NcSpindleMode::Stopped,
        tool_number:None,
    };
    let trace=interpret_program(NC,initial).map_err(|e|e.to_string())?;
    segments_from_trace(&trace,Ok)
}
fn error_all(a:&Sparse,b:&MaterialHeightfield)->f64 {
    let mut worst=0.0_f64;
    for iy in 0..a.nx {
        for ix in 0..a.nx {
            worst=worst.max((a.height(ix,iy) as f64-b.height_at(ix,iy)).abs());
        }
    }
    worst
}
fn run(shape:CutterShape,step:f64,segments:&[CutSegment])->Result<(),String>{
    let n=(SIDE/step).round() as usize+1;
    let start=Instant::now();
    let mut sparse=Sparse::new(step)?;
    let mut reference=MaterialHeightfield::simulate_with_budget(
        StockBox{min_mm:[0.0;3],size_mm:[SIDE,SIDE,HEIGHT]},
        CutterSpec{shape,diameter_mm:DIAM,cutting_length_mm:12.0},
        n,512,segments,&AtomicBool::new(false),&AtomicUsize::new(0)
    )?;
    let mut worst=0.0_f64;
    let mut intermediate_samples=0_usize;
    // Comprobar cada movimiento interpretado, incluido G0 y M5.
    for (i,seg) in segments.iter().enumerate() {
        sparse.apply(seg,shape,1.0)?;
        reference.seek((i+1) as f64);
        let diff=error_all(&sparse,&reference);
        worst=worst.max(diff);
        if diff>EPS {return Err(format!("Segmento NC {} (linea {}): diferencia {diff:.7} mm",i+1,seg.nc_line));}
        // Chequeo fraccional: aplicar solo sellos con marca t<=0.5 en el segmento,
        // igual que el seek del motor de referencia.
        if seg.spindle_on {
            let halfway=(sparse.segment_samples(seg) as f64*0.5).floor() as usize;
            let fraction=halfway as f64/sparse.segment_samples(seg) as f64;
            sparse.undo()?;
            sparse.apply(seg,shape,fraction)?;
            reference.seek(i as f64+fraction);
            let half_err=error_all(&sparse,&reference);
            worst=worst.max(half_err);
            intermediate_samples+=1;
            if half_err>EPS {return Err(format!("Mitad NC {}: diferencia {half_err:.7} mm",i+1));}
            sparse.undo()?;
            sparse.apply(seg,shape,1.0)?;
            reference.seek((i+1) as f64);
        }
    }
    let finished=error_all(&sparse,&reference);
    worst=worst.max(finished);
    if worst>EPS {return Err(format!("Error final {worst:.7} mm"));}
    let tiles=sparse.tiles.len();
    let tile_mb=sparse.mb();
    let history_mb=sparse.history_mb();
    let elapsed=start.elapsed().as_secs_f64();
    // Rebobinar todos los movimientos NC y verificar bruto limpio.
    for _ in segments {sparse.undo()?;}
    if !sparse.tiles.is_empty() {return Err("Rebobinado incompleto: quedaron bloques".into());}
    reference.seek(0.0);
    if error_all(&sparse,&reference)>EPS {return Err("Inicio no coincide con el motor original".into());}
    // Repetir movimientos: la malla final debe coincidir exactamente.
    for seg in segments {sparse.apply(seg,shape,1.0)?;}
    reference.seek(segments.len() as f64);
    if error_all(&sparse,&reference)>EPS {return Err("Reproduccion final distinta".into());}
    println!("{:>8} | {:>5.2} | {:>4} | {:>4} | {:>6} | {:>8.2} | {:>8.2} | {:>8.3} | {:>10.7} | OK",
        match shape{CutterShape::Flat=>"plana",CutterShape::Ball=>"esferica"},
        step,segments.len(),intermediate_samples,tiles,tile_mb,history_mb,elapsed,worst);
    Ok(())
}
fn main()->Result<(),String>{
    let segments=program()?;
    println!("LAGUCAM V11 - NC interpretado con motor REAL y material por bloques");
    println!("Programa: {} movimientos, G0/G1 + M3/M5; reproduccion por segmento y fraccion",segments.len());
    for (i,s) in segments.iter().enumerate() {
        println!("  {:>2}: linea {:>2}, {} {}, desde {:?}, hasta {:?}",
            i+1,s.nc_line,if s.rapid{"G0"}else{"G1"},
            if s.spindle_on{"husillo ON"}else{"husillo OFF"},
            s.from_model_mm,s.to_model_mm);
    }
    println!("Cada estado NC se compara contra MaterialHeightfield (malla completa).");
    println!("Fresa    | Paso  | Movs | Meds | Bloq   | Bloq MiB | Hist MiB | Tiempo s | Error mm   | Estado");
    for step in [0.2,0.1] {
        for shape in [CutterShape::Flat,CutterShape::Ball] {
            run(shape,step,&segments)?;
        }
    }
    println!("FIN: undo por movimiento completo y verificacion fraccional; SIN OpenGL.");
    println!("No demuestra memoria escalable para NC largos ni reproduccion interactiva a 0.05 mm.");
    Ok(())
}
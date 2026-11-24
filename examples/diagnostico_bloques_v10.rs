//! LaguCAM V10: prueba aislada de superficie dispersa + historial por operacion.
//! No representa G-code ni renderiza; no modifica el simulador de produccion.
use cam_libre_visor::material_sim::{CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Instant;

const TILE: usize = 128;
const TOP: f32 = 10.0;
const RADIUS: f64 = 2.5;
type Key = (usize, usize);
type Data = Box<[f32]>;

#[derive(Clone, Copy, Debug)]
enum Tool { Flat, Ball }
impl Tool {
    fn label(self) -> &'static str { match self { Self::Flat=>"plana", Self::Ball=>"esferica" } }
    fn bottom(self, r2:f64)->f64 { match self { Self::Flat=>0.0, Self::Ball=>RADIUS-(RADIUS*RADIUS-r2).max(0.0).sqrt() } }
    fn cutter(self)->CutterShape { match self { Self::Flat=>CutterShape::Flat, Self::Ball=>CutterShape::Ball } }
}
#[derive(Clone,Copy)]
struct Op { a:[f64;2], b:[f64;2], tip_z:f64 }
fn operations(size:f64)->Vec<Op> {
    let mut result:Vec<Op>=(0..9).map(|i| {
        let y=size*(0.1+0.8*(i as f64+0.5)/9.0);
        let x0=size*0.1; let x1=size*0.9;
        let (a,b)=if i%2==0 {([x0,y],[x1,y])} else {([x1,y],[x0,y])};
        Op {a,b,tip_z:5.0}
    }).collect();
    // Corte adicional mas profundo sobre la primera pasada: fuerza una restauracion real.
    result.push(Op {tip_z:4.5, ..result[0]});
    result
}
struct Before { key:Key, old:Option<Data> }
struct Snapshot { before:Vec<Before>, bytes:usize }
struct Surface {
    side:f64, pitch:f64, n:usize, tiles:HashMap<Key,Data>,
    undo:Vec<Snapshot>, writes:u64,
}
impl Surface {
    fn new(side:f64,pitch:f64)->Result<Self,String> {
        if !(side.is_finite() && pitch.is_finite() && side>0.0 && pitch>0.0) {return Err("Parametros invalidos".into())}
        let samples=side/pitch;
        if samples>100_000_000.0 || (samples-samples.round()).abs()>1e-7 {return Err("El lado debe ser multiplo del paso".into())}
        Ok(Self {side,pitch,n:samples.round() as usize+1,tiles:HashMap::new(),undo:vec![],writes:0})
    }
    fn get(&self,x:usize,y:usize)->f32 {
        assert!(x<self.n && y<self.n);
        self.tiles.get(&(x/TILE,y/TILE)).map(|v|v[(y%TILE)*TILE+x%TILE]).unwrap_or(TOP)
    }
    fn lower(&mut self,x:usize,y:usize,z:f32) {
        if z>=TOP {return;}
        let v=self.tiles.entry((x/TILE,y/TILE)).or_insert_with(||vec![TOP;TILE*TILE].into_boxed_slice());
        let index=(y%TILE)*TILE+x%TILE;
        if z<v[index] {v[index]=z;self.writes+=1;}
    }
    fn region(&self,op:Op)->(usize,usize,usize,usize) {
        let left=op.a[0].min(op.b[0])-RADIUS;
        let right=op.a[0].max(op.b[0])+RADIUS;
        let low=op.a[1].min(op.b[1])-RADIUS;
        let high=op.a[1].max(op.b[1])+RADIUS;
        let idx=|mm:f64,ceil:bool| ->usize {
            let v=(mm/self.pitch).clamp(0.0,(self.n-1) as f64);
            if ceil {v.ceil() as usize} else {v.floor() as usize}
        };
        (idx(left,false),idx(right,true),idx(low,false),idx(high,true))
    }
    fn apply(&mut self,op:Op,tool:Tool) {
        let (x0,x1,y0,y1)=self.region(op);
        let mut before=Vec::new();let mut bytes=0;
        for ty in y0/TILE..=y1/TILE {
            for tx in x0/TILE..=x1/TILE {
                let key=(tx,ty);
                let old=self.tiles.get(&key).cloned();
                if old.is_some(){bytes+=TILE*TILE*std::mem::size_of::<f32>();}
                before.push(Before{key,old});
            }
        }
        let dx=op.b[0]-op.a[0];let dy=op.b[1]-op.a[1];let len2=dx*dx+dy*dy;
        for iy in y0..=y1 {
            let y=iy as f64*self.pitch;
            for ix in x0..=x1 {
                let x=ix as f64*self.pitch;
                let t=if len2==0.0 {0.0} else {(((x-op.a[0])*dx+(y-op.a[1])*dy)/len2).clamp(0.0,1.0)};
                let rx=x-(op.a[0]+t*dx);let ry=y-(op.a[1]+t*dy);
                let r2=rx*rx+ry*ry;
                if r2<=RADIUS*RADIUS {self.lower(ix,iy,(op.tip_z+tool.bottom(r2)) as f32);}
            }
        }
        self.undo.push(Snapshot {before,bytes});
    }
    fn undo_one(&mut self)->Result<(),String> {
        let saved=self.undo.pop().ok_or("No hay operaciones que deshacer")?;
        for change in saved.before {
            match change.old {Some(old)=>{self.tiles.insert(change.key,old);},None=>{self.tiles.remove(&change.key);} }
        }
        Ok(())
    }
    fn tiles_mib(&self)->f64 {self.tiles.len() as f64 * (TILE*TILE*4) as f64/1048576.0}
    fn history_mib(&self)->f64 {self.undo.iter().map(|x|x.bytes).sum::<usize>() as f64/1048576.0}
    fn fingerprint(&self)->u64 {
        // Comparacion exacta, determinista, de todas las alturas de todos los bloques.
        let mut keys:Vec<_>=self.tiles.keys().copied().collect();keys.sort_unstable();
        let mut h=14695981039346656037u64;
        for key in keys {
            for part in [key.0 as u64,key.1 as u64] {h=(h^part).wrapping_mul(1099511628211);}
            for &height in self.tiles[&key].iter() {
                h=(h^height.to_bits() as u64).wrapping_mul(1099511628211);
            }
        }
        h
    }
}
fn independent_z(size:f64,pitch:f64,ix:usize,iy:usize,tool:Tool)->f32 {
    let p=[ix as f64*pitch,iy as f64*pitch];
    let mut value=TOP as f64;
    for op in operations(size) {
        let dx=op.b[0]-op.a[0];let dy=op.b[1]-op.a[1];let len2=dx*dx+dy*dy;
        let t=if len2==0.0 {0.0} else {(((p[0]-op.a[0])*dx+(p[1]-op.a[1])*dy)/len2).clamp(0.0,1.0)};
        let x=p[0]-(op.a[0]+dx*t);let y=p[1]-(op.a[1]+dy*t);let r2=x*x+y*y;
        if r2<=RADIUS*RADIUS {value=value.min(op.tip_z+tool.bottom(r2));}
    }
    value as f32
}
fn verify(surface:&Surface,kind:Tool,seed:u64)->f64 {
    // 50 mil puntos distribuidos por el bruto y muestreo adicional en las diez franjas.
    let mut worst=0.0f64;
    let mut rng=seed;
    for i in 0..50_000 {
        rng=rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let x=((rng>>32) as usize)%surface.n;
        rng=rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut y=((rng>>32) as usize)%surface.n;
        if i%2==0 {
            let op=operations(surface.side)[i%10];
            let target=((op.a[1]+((i%31) as f64-15.0)*surface.pitch)/surface.pitch).round();
            y=target.clamp(0.0,(surface.n-1) as f64) as usize;
        }
        worst=worst.max((surface.get(x,y)-independent_z(surface.side,surface.pitch,x,y,kind)).abs() as f64);
    }
    worst
}
fn compare_motor(kind:Tool)->Result<f64,String> {
    let size=100.0;let pitch=0.1;let n=1001;
    let ops=operations(size);
    let segments:Vec<CutSegment>=ops.iter().enumerate().map(|(i,op)|CutSegment {
        from_model_mm:[op.a[0],op.a[1],op.tip_z],
        to_model_mm:[op.b[0],op.b[1],op.tip_z],spindle_on:true,rapid:false,nc_line:i+1
    }).collect();
    let mut reference=MaterialHeightfield::simulate_with_budget(
        StockBox{min_mm:[0.0;3],size_mm:[size,size,10.0]},
        CutterSpec{shape:kind.cutter(),diameter_mm:5.0,cutting_length_mm:12.0},
        n,512,&segments,&AtomicBool::new(false),&AtomicUsize::new(0)
    )?;
    reference.seek(ops.len() as f64);
    let mut sparse=Surface::new(size,pitch)?;
    for op in ops {sparse.apply(op,kind);}
    let mut worst=0.0f64;
    for iy in 0..n {for ix in 0..n {
        worst=worst.max((reference.height_at(ix,iy)-sparse.get(ix,iy) as f64).abs());
    }}
    Ok(worst)
}
fn test(size:f64,pitch:f64,kind:Tool)->Result<(),String> {
    let ops=operations(size);
    let mut s=Surface::new(size,pitch)?;
    let start=Instant::now();
    for op in ops.iter().copied() {s.apply(op,kind);}
    let seconds=start.elapsed().as_secs_f64();
    let mb=s.tiles_mib();let hist=s.history_mib();
    let f1=s.fingerprint();
    let err=verify(&s,kind,0x1234567890abcdef);
    if err>0.00001 {return Err(format!("ERROR geometrico {size} mm {}: {err:.7}",kind.label()));}
    for _ in 0..ops.len(){s.undo_one()?;}
    if !s.tiles.is_empty() {return Err("Undo no recupero el bruto original".into());}
    for op in ops {s.apply(op,kind);}
    if s.fingerprint()!=f1 {return Err("Redo no reproduce exactamente las mismas alturas".into());}
    let full_mib=(s.n as f64)*(s.n as f64)*4.0/1048576.0;
    println!("{:>5.0} | {:>5.2} | {:>8} | {:>7} | {:>8.2} | {:>8.2} | {:>9.1} | {:>7.3} | {:>9.6} | OK",
        size,pitch,kind.label(),s.tiles.len(),mb,hist,full_mib,seconds,err);
    Ok(())
}
fn main()->Result<(),String> {
    println!("LAGUCAM V10 - Bloques dispersos + snapshots por operacion + rollback/redo");
    println!("Diez operaciones, 9 pasadas repartidas en el 80% del bruto + pasada 10 solapada mas profunda.");
    println!("RAM bloques/historial = datos f32 estimados, no RAM total del proceso. SIN OpenGL ni NC real.");
    println!("Bruto | Paso | Fresa    | Bloques | Bloq MiB | Hist MiB | Densa MiB | T(s)    | Error mm  | Estado");
    for size in [100.0,500.0,1000.0] {
        for kind in [Tool::Flat,Tool::Ball] {test(size,0.05,kind)?;}
    }
    println!("Comparando toda la malla 1001x1001, bruto 100 mm a 0.10 mm, contra material_sim real...");
    for kind in [Tool::Flat,Tool::Ball] {
        let t=Instant::now();
        let error=compare_motor(kind)?;
        println!("Motor real {}: desviacion maxima {:.6} mm, {:.3} s | {}",
            kind.label(),error,t.elapsed().as_secs_f64(),
            if error<=0.02 {"DENTRO TOLERANCIA DIAGNOSTICA 0.02 mm"} else {"REVISAR"});
    }
    println!("FIN - no se almacena movimiento continuo dentro de cada operacion; retroceso por operaciones completas.");
    Ok(())
}
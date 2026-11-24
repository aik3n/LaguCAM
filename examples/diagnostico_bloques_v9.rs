//! LaguCAM: experimento aislado de almacenamiento 2.5D por bloques dispersos.
//! No interpreta NC, no implementa historial temporal ni pinta con OpenGL.
//! Objetivo: mismo paso espacial (mm) sin reservar toda la superficie del bruto.
use std::collections::HashMap;
use std::time::Instant;

const TILE: usize = 128;
const TOP: f32 = 10.0;
const TIP_Z: f64 = 5.0;
const RADIUS: f64 = 2.5;
const PASSES: usize = 9;

#[derive(Clone, Copy, Debug)]
enum Tool {
    Flat,
    Ball,
}
impl Tool {
    fn label(self) -> &'static str {
        match self { Self::Flat => "plana", Self::Ball => "esferica" }
    }
    fn bottom(self, r2: f64) -> f64 {
        match self {
            Self::Flat => 0.0,
            Self::Ball => RADIUS - (RADIUS * RADIUS - r2).max(0.0).sqrt(),
        }
    }
}

struct SparseStock {
    size: f64,
    step: f64,
    nodes: usize,
    // Cada bloque contiene 128x128 alturas relativas al bruto (f32).
    tiles: HashMap<(usize, usize), Box<[f32]>>,
    modified_samples: u64,
}
impl SparseStock {
    fn new(size: f64, step: f64) -> Result<Self, String> {
        if !(size.is_finite() && step.is_finite() && size > 0.0 && step > 0.0) {
            return Err("Tamano o paso no validos".into());
        }
        let rounded = size / step;
        if rounded > 100_000_000.0 || (rounded - rounded.round()).abs() > 1.0e-7 {
            return Err("Esta prueba requiere tamano multiplo exacto del paso".into());
        }
        let nodes = rounded.round() as usize + 1;
        Ok(Self { size, step, nodes, tiles: HashMap::new(), modified_samples: 0 })
    }
    fn height(&self, ix: usize, iy: usize) -> f32 {
        assert!(ix < self.nodes && iy < self.nodes);
        let key = (ix / TILE, iy / TILE);
        self.tiles.get(&key)
            .map(|data| data[(iy % TILE) * TILE + (ix % TILE)])
            .unwrap_or(TOP)
    }
    fn lower(&mut self, ix: usize, iy: usize, z: f32) {
        if z >= TOP { return; }
        let key = (ix / TILE, iy / TILE);
        // Solo se reserva el tile si realmente se elimina material.
        let data = self.tiles.entry(key)
            .or_insert_with(|| vec![TOP; TILE * TILE].into_boxed_slice());
        let at = (iy % TILE) * TILE + (ix % TILE);
        if z < data[at] {
            data[at] = z;
            self.modified_samples += 1;
        }
    }
    fn cut_line(&mut self, kind: Tool, from: [f64; 2], to: [f64; 2]) {
        let xmin = (from[0].min(to[0]) - RADIUS).max(0.0);
        let xmax = (from[0].max(to[0]) + RADIUS).min(self.size);
        let ymin = (from[1].min(to[1]) - RADIUS).max(0.0);
        let ymax = (from[1].max(to[1]) + RADIUS).min(self.size);
        let ix0 = (xmin / self.step).floor() as usize;
        let ix1 = ((xmax / self.step).ceil() as usize).min(self.nodes - 1);
        let iy0 = (ymin / self.step).floor() as usize;
        let iy1 = ((ymax / self.step).ceil() as usize).min(self.nodes - 1);
        let delta = [to[0] - from[0], to[1] - from[1]];
        let length2 = delta[0] * delta[0] + delta[1] * delta[1];
        for iy in iy0..=iy1 {
            let y = iy as f64 * self.step;
            for ix in ix0..=ix1 {
                let x = ix as f64 * self.step;
                let t = if length2 > 0.0 {
                    (((x - from[0]) * delta[0] + (y - from[1]) * delta[1]) / length2)
                        .clamp(0.0, 1.0)
                } else { 0.0 };
                let dx = x - (from[0] + t * delta[0]);
                let dy = y - (from[1] + t * delta[1]);
                let r2 = dx * dx + dy * dy;
                if r2 <= RADIUS * RADIUS {
                    self.lower(ix, iy, (TIP_Z + kind.bottom(r2)) as f32);
                }
            }
        }
    }
    fn memory_mib(&self) -> f64 {
        self.tiles.len() as f64 * (TILE * TILE * std::mem::size_of::<f32>()) as f64
            / 1_048_576.0
    }
}
fn lines(size: f64) -> Vec<([f64; 2], [f64; 2])> {
    (0..PASSES).map(|i| {
        let y = size / 2.0 - 8.0 + i as f64 * 2.0;
        let x0 = size / 2.0 - 30.0;
        let x1 = size / 2.0 + 30.0;
        if i % 2 == 0 { ([x0,y], [x1,y]) }
        else { ([x1,y], [x0,y]) }
    }).collect()
}
fn expected_height(size: f64, step: f64, ix: usize, iy: usize, tool: Tool) -> f32 {
    let x = ix as f64 * step;
    let y = iy as f64 * step;
    let mut height = TOP as f64;
    for (from,to) in lines(size) {
        let dx = if x < from[0].min(to[0]) { x - from[0].min(to[0]) }
                 else if x > from[0].max(to[0]) { x - from[0].max(to[0]) }
                 else { 0.0 };
        let dy = y - from[1];
        let r2 = dx*dx + dy*dy;
        if r2 <= RADIUS*RADIUS {
            height = height.min(TIP_Z + tool.bottom(r2));
        }
    }
    height as f32
}
fn run(size: f64, step: f64, kind: Tool) -> Result<(), String> {
    let mut stock = SparseStock::new(size, step)?;
    let began = Instant::now();
    for (a,b) in lines(size) { stock.cut_line(kind,a,b); }
    let elapsed = began.elapsed().as_secs_f64();
    // Incluye puntos de extremos, zona mecanizada, fuera de ella y muestras
    // reproducibles repartidas por todo el bruto.
    let mut max_error: f64 = 0.0;
    let mut tested = 0_u64;
    let mid = stock.nodes / 2;
    let around = (RADIUS / step).ceil() as isize + 3;
    for iy in (mid as isize - 4*around)..=(mid as isize + 4*around) {
        if iy < 0 || iy >= stock.nodes as isize {continue;}
        for ix in (mid as isize - around)..=(mid as isize + around) {
            if ix < 0 || ix >= stock.nodes as isize {continue;}
            let actual = stock.height(ix as usize, iy as usize);
            let expected = expected_height(size, step, ix as usize, iy as usize, kind);
            max_error = max_error.max((actual - expected).abs() as f64);
            tested += 1;
        }
    }
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..20_000 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ix = ((rng >> 32) as usize) % stock.nodes;
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let iy = ((rng >> 32) as usize) % stock.nodes;
        let actual = stock.height(ix,iy);
        let expected = expected_height(size,step,ix,iy,kind);
        max_error = max_error.max((actual - expected).abs() as f64);
        tested += 1;
    }
    let dense_mib = (stock.nodes as f64).powi(2) * 4.0 / 1_048_576.0;
    println!("{:>5.0} | {:>5.3} | {:>8} | {:>8} | {:>8.2} | {:>9.2} | {:>7.3} | {:>8} | {:>7.5} | {}",
        size,step,kind.label(),stock.tiles.len(),stock.memory_mib(),dense_mib,
        elapsed,stock.modified_samples,max_error,
        if max_error <= 1.0e-5 {"OK"} else {"REVISAR"});
    if max_error > 1.0e-5 {
        return Err(format!("Geometria incorrecta en bruto {size}, herramienta {}",kind.label()));
    }
    if tested == 0 { return Err("Sin muestras de verificacion".into()); }
    Ok(())
}
fn main() -> Result<(),String> {
    println!("LAGUCAM V9 - ALMACENAMIENTO POR BLOQUES, PASO FISICO CONSTANTE");
    println!("Tile 128x128, alturas f32, reserva dispersa; 9 pasadas rectas, Ø5, Z5.");
    println!("Comparacion analitica independiente por puntos, NO motor NC ni OpenGL.");
    println!("Bruto | Paso  | Fresa    | Bloques  | RAM MiB | Densa MiB | Tiempo s | Cambios  | Error mm | Estado");
    for size in [100.0,500.0,1000.0] {
        for kind in [Tool::Flat,Tool::Ball] {run(size,0.05,kind)?;}
    }
    println!("FIN: solo almacena superficie final, sin reproducir/retroceder por tiempo.");
    Ok(())
}
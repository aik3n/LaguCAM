//! Motor experimental de material 2.5D disperso, independiente de la UI.
//!
//! Una altura f32 por muestra XY, con paso espacial en mm, no por lado.
//! Conserva la traza NC interpretada (CutSegment) como unica fuente de movimientos.
//! No representa socavados, cavidades cerradas ni mecanizado de 5 ejes.
//! Esta fase NO sustituye material_sim en la UI.
use crate::material_sim::{CutSegment, CutterShape, CutterSpec, MAX_SIM_MOVES, StockBox};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const TILE: usize = 128;
const SAMPLES_PER_TILE: usize = TILE * TILE;
const TILE_BYTES: usize = SAMPLES_PER_TILE * std::mem::size_of::<f32>();
const MAX_CELLS_PER_AXIS: usize = 20_000_001;

type Key = (usize, usize);
type Tile = Arc<[f32]>;
type TileMap = HashMap<Key, Tile>;

/// Rectangulo de alturas modificado, medido en muestras XY del bruto.
/// `values` contiene `width * height` alturas RELATIVAS a Z minima,
/// en orden de filas. La coordenada global del primer pixel es (x, y).
/// El consumidor debe aplicar regiones con version de cursor NC coherente.
#[derive(Clone, Debug)]
pub struct MaterialRegion {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct SparseConfig {
    pub stock: StockBox,
    pub cutter: CutterSpec,
    /// Paso maximo en XY en milimetros, independiente del tamano del bruto.
    pub spacing_mm: f64,
    /// Separacion en movimientos entre puntos de control.
    pub checkpoint_every: usize,
    /// Numero maximo de puntos de control retenidos.
    pub max_checkpoints: usize,
    /// Presupuesto de alturas (estado actual + snapshots) en MiB.
    /// Si el estado activo por si mismo lo supera, la operacion devuelve error.
    pub height_budget_mib: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SparseStats {
    pub tiles: usize,
    pub checkpoints: usize,
    pub height_bytes: usize,
    pub replayed_moves: usize,
    pub cursor: f64,
}

#[derive(Clone)]
struct Snapshot {
    completed: usize,
    tiles: TileMap,
}

pub struct SparseMaterial {
    config: SparseConfig,
    cells: [usize; 2],
    spacing: [f64; 2],
    tiles: TileMap,
    dirty_tiles: HashSet<Key>,
    snapshots: Vec<Snapshot>,
    cursor: f64,
    replayed_moves: usize,
}

impl SparseMaterial {
    pub fn new(config: SparseConfig) -> Result<Self, String> {
        config.stock.validate()?;
        config.cutter.validate()?;
        if !config.spacing_mm.is_finite() || config.spacing_mm <= 0.0 {
            return Err("Resolucion espacial invalida".into());
        }
        if config.checkpoint_every == 0
            || config.max_checkpoints == 0
            || config.height_budget_mib == 0
        {
            return Err("Configuracion de checkpoints o presupuesto invalida".into());
        }
        let mut cells = [0; 2];
        let mut spacing = [0.0; 2];
        for axis in 0..2 {
            let intervals = (config.stock.size_mm[axis] / config.spacing_mm).ceil();
            if !intervals.is_finite() || intervals < 1.0 || intervals >= MAX_CELLS_PER_AXIS as f64 {
                return Err("Tamano de malla fuera del rango permitido".into());
            }
            cells[axis] = intervals as usize + 1;
            spacing[axis] = config.stock.size_mm[axis] / (cells[axis] - 1) as f64;
        }
        Ok(Self {
            config,
            cells,
            spacing,
            tiles: HashMap::new(),
            dirty_tiles: HashSet::new(),
            snapshots: Vec::new(),
            cursor: 0.0,
            replayed_moves: 0,
        })
    }

    pub fn fresh(&self) -> Result<Self, String> {
        Self::new(self.config)
    }

    pub fn stock(&self) -> StockBox {
        self.config.stock
    }
    pub fn cells(&self) -> [usize; 2] {
        self.cells
    }
    pub fn spacing_mm(&self) -> [f64; 2] {
        self.spacing
    }

    pub fn height_at(&self, ix: usize, iy: usize) -> Option<f64> {
        if ix >= self.cells[0] || iy >= self.cells[1] {
            return None;
        }
        let top = self.config.stock.size_mm[2] as f32;
        let rel = self
            .tiles
            .get(&(ix / TILE, iy / TILE))
            .map(|tile| tile[(iy % TILE) * TILE + ix % TILE])
            .unwrap_or(top);
        Some(self.config.stock.min_mm[2] + rel as f64)
    }

    /// Devuelve UNICAMENTE los bloques cuyo material cambio desde la ultima lectura.
    /// Al retroceder tambien incluye bloques que deben restaurarse a bruto intacto.
    /// No lee la rejilla completa y no altera la resolucion de calculo.
    pub fn take_dirty_regions(&mut self) -> Vec<MaterialRegion> {
        let mut keys: Vec<_> = std::mem::take(&mut self.dirty_tiles).into_iter().collect();
        keys.sort_unstable();
        let top = self.config.stock.size_mm[2] as f32;
        let mut output = Vec::with_capacity(keys.len());
        for (tx, ty) in keys {
            let x = tx * TILE;
            let y = ty * TILE;
            if x >= self.cells[0] || y >= self.cells[1] {
                continue;
            }
            let width = TILE.min(self.cells[0] - x);
            let height = TILE.min(self.cells[1] - y);
            let mut values = Vec::with_capacity(width * height);
            if let Some(tile) = self.tiles.get(&(tx, ty)) {
                for row in 0..height {
                    let start = row * TILE;
                    values.extend_from_slice(&tile[start..start + width]);
                }
            } else {
                values.resize(width * height, top);
            }
            output.push(MaterialRegion {
                x,
                y,
                width,
                height,
                values,
            });
        }
        output
    }

    fn height_bytes(&self) -> usize {
        let mut unique = HashSet::new();
        for tile in self.tiles.values() {
            unique.insert(tile.as_ptr() as *const f32 as usize);
        }
        for snapshot in &self.snapshots {
            for tile in snapshot.tiles.values() {
                unique.insert(tile.as_ptr() as *const f32 as usize);
            }
        }
        unique.len().saturating_mul(TILE_BYTES)
    }

    pub fn stats(&self) -> SparseStats {
        SparseStats {
            tiles: self.tiles.len(),
            checkpoints: self.snapshots.len(),
            height_bytes: self.height_bytes(),
            replayed_moves: self.replayed_moves,
            cursor: self.cursor,
        }
    }

    fn budget_bytes(&self) -> usize {
        self.config.height_budget_mib.saturating_mul(1024 * 1024)
    }

    fn trim_snapshots(&mut self) -> Result<(), String> {
        while (self.snapshots.len() > self.config.max_checkpoints
            || self.height_bytes() > self.budget_bytes())
            && !self.snapshots.is_empty()
        {
            self.snapshots.remove(0);
        }
        if self.height_bytes() > self.budget_bytes() {
            return Err("El estado activo supera el presupuesto de alturas".into());
        }
        Ok(())
    }

    fn remember(&mut self, completed: usize) -> Result<(), String> {
        if !self
            .snapshots
            .iter()
            .any(|snap| snap.completed == completed)
        {
            self.snapshots.push(Snapshot {
                completed,
                tiles: self.tiles.clone(),
            });
            self.snapshots.sort_unstable_by_key(|snap| snap.completed);
        }
        self.trim_snapshots()
    }

    /// Envolvente geometrica CONTINUA del segmento NC entre dos fracciones.
    /// Cada altura XY se visita a lo sumo una vez por intervalo del segmento.
    /// No depende de M3/M4/M5 ni del tipo de movimiento (G0/G1).
    fn apply(
        &mut self,
        segment: &CutSegment,
        from_fraction: f64,
        to_fraction: f64,
    ) -> Result<(), String> {
        let delta: [f64; 3] =
            std::array::from_fn(|i| segment.to_model_mm[i] - segment.from_model_mm[i]);
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(format!(
                "Linea {}: movimiento NC no finito",
                segment.nc_line
            ));
        }
        let a: [f64; 3] =
            std::array::from_fn(|i| segment.from_model_mm[i] + delta[i] * from_fraction);
        let b: [f64; 3] =
            std::array::from_fn(|i| segment.from_model_mm[i] + delta[i] * to_fraction);
        // Parametrizacion CANONICA del movimiento NC completo, incluso cuando
        // el trabajador ejecuta unicamente un tramo fraccionario. Reparametrizar
        // cada tramo cambiaba la precision numerica cerca de la tangencia.
        let dx = delta[0];
        let dy = delta[1];
        let dz = delta[2];
        let stock = self.config.stock;
        let cutter = self.config.cutter;
        let r = cutter.diameter_mm * 0.5;
        let r2 = r * r;
        let top = stock.size_mm[2] as f32;
        let absolute_top = stock.min_mm[2] + stock.size_mm[2];
        if a[2].min(b[2]) >= absolute_top
            || a[2].max(b[2]) + cutter.cutting_length_mm <= stock.min_mm[2]
        {
            return Ok(());
        }
        let lo_x = a[0].min(b[0]) - r;
        let hi_x = a[0].max(b[0]) + r;
        let lo_y = a[1].min(b[1]) - r;
        let hi_y = a[1].max(b[1]) + r;
        if hi_x < stock.min_mm[0]
            || lo_x > stock.min_mm[0] + stock.size_mm[0]
            || hi_y < stock.min_mm[1]
            || lo_y > stock.min_mm[1] + stock.size_mm[1]
        {
            return Ok(());
        }
        let index = |axis: usize, p: f64, upper: bool| {
            let q = ((p - stock.min_mm[axis]) / self.spacing[axis])
                .clamp(0.0, (self.cells[axis] - 1) as f64);
            if upper {
                q.ceil() as usize
            } else {
                q.floor() as usize
            }
        };
        let x0 = index(0, lo_x, false);
        let x1 = index(0, hi_x, true);
        let y0 = index(1, lo_y, false);
        let y1 = index(1, hi_y, true);
        let horizontal2 = dx * dx + dy * dy;
        let horizontal = horizontal2.sqrt();
        let axial_slope = if horizontal > 1e-8 {
            dz / horizontal
        } else {
            0.0
        };
        let slope_div = (1.0 + axial_slope * axial_slope).sqrt();
        for iy in y0..=y1 {
            let py = stock.min_mm[1] + iy as f64 * self.spacing[1] - segment.from_model_mm[1];
            for ix in x0..=x1 {
                let px = stock.min_mm[0] + ix as f64 * self.spacing[0] - segment.from_model_mm[0];
                let min_height = if horizontal2 <= 1e-16 {
                    let rr = px * px + py * py;
                    if rr > r2 {
                        continue;
                    }
                    let bowl = match cutter.shape {
                        CutterShape::Flat => 0.0,
                        CutterShape::Ball => r - (r2 - rr).max(0.0).sqrt(),
                    };
                    a[2].min(b[2]) + bowl
                } else {
                    let t0 = (px * dx + py * dy) / horizontal2;
                    let perpendicular2 = (px * px + py * py - t0 * t0 * horizontal2).max(0.0);
                    let q2 = r2 - perpendicular2;
                    if q2 < -1e-10 {
                        continue;
                    }
                    let q = q2.max(0.0).sqrt();
                    let window = q / horizontal;
                    // Restringir el intervalo de contacto AL TRAMO solicitado,
                    // pero en coordenadas globales de la linea NC [0, 1].
                    let lower = (t0 - window).clamp(from_fraction, to_fraction);
                    let upper = (t0 + window).clamp(from_fraction, to_fraction);
                    if lower > upper {
                        continue;
                    }
                    let t = match cutter.shape {
                        CutterShape::Flat => {
                            if dz >= 0.0 {
                                lower
                            } else {
                                upper
                            }
                        }
                        CutterShape::Ball => {
                            let optimal = t0 - (axial_slope * q / slope_div) / horizontal;
                            optimal.clamp(lower, upper)
                        }
                    };
                    let residual_x = px - dx * t;
                    let residual_y = py - dy * t;
                    let rr = residual_x * residual_x + residual_y * residual_y;
                    // Clamp() puede alcanzar el extremo temporal aunque el
                    // disco no lo alcance: validar el radio de contacto real.
                    // Tolerancia numerica submicrometrica para tangencias exactas.
                    if rr > r2 + 1.0e-10 {
                        continue;
                    }
                    let bowl = match cutter.shape {
                        CutterShape::Flat => 0.0,
                        CutterShape::Ball => r - (r2 - rr).max(0.0).sqrt(),
                    };
                    segment.from_model_mm[2] + dz * t + bowl
                };
                let z = ((min_height - stock.min_mm[2]).clamp(0.0, stock.size_mm[2])) as f32;
                if z >= top {
                    continue;
                }
                let entry = self
                    .tiles
                    .entry((ix / TILE, iy / TILE))
                    .or_insert_with(|| Arc::from(vec![top; SAMPLES_PER_TILE]));
                let pixel = (iy % TILE) * TILE + (ix % TILE);
                if z < entry[pixel] {
                    Arc::make_mut(entry)[pixel] = z;
                    self.dirty_tiles.insert((ix / TILE, iy / TILE));
                }
            }
        }
        Ok(())
    }
    /// Posicion de reproduccion: parte entera = movimientos completos,
    /// fraccion = parte del movimiento siguiente.
    /// El estado anterior permanece sin cambios si falla la validacion.
    pub fn seek(&mut self, segments: &[CutSegment], target: f64) -> Result<(), String> {
        if segments.len() > MAX_SIM_MOVES
            || !target.is_finite()
            || !(0.0..=segments.len() as f64).contains(&target)
        {
            return Err("Posicion de reproduccion NC invalida".into());
        }
        for segment in segments {
            if segment
                .from_model_mm
                .iter()
                .chain(segment.to_model_mm.iter())
                .any(|v| !v.is_finite())
            {
                return Err(format!(
                    "Linea {}: coordenadas NC invalidas",
                    segment.nc_line
                ));
            }
        }
        if target == self.cursor {
            return Ok(());
        }
        // Un seek hacia atras invalida las regiones previamente modificadas.
        let rewinding = target < self.cursor;
        if rewinding {
            self.dirty_tiles.extend(self.tiles.keys().copied());
        }
        let completed = target.floor() as usize;
        let fraction = target - completed as f64;

        // Las alturas solo disminuyen: las aplicaciones repetidas de una misma
        // fraccion de barrido son idempotentes. Si avanzamos, continuar desde
        // el estado actual incluso cuando el cursor anterior es fraccionario.
        // Solo rebobinar desde checkpoint si el cursor retrocede.
        let forward = target >= self.cursor;
        let baseline = if forward {
            self.cursor.floor() as usize
        } else {
            let best = self
                .snapshots
                .iter()
                .filter(|snap| snap.completed <= completed)
                .max_by_key(|snap| snap.completed);
            if let Some(snapshot) = best {
                self.tiles = snapshot.tiles.clone();
                snapshot.completed
            } else {
                self.tiles.clear();
                0
            }
        };
        self.replayed_moves = completed.saturating_sub(baseline);
        for index in baseline..completed {
            let start = if forward && index == baseline {
                self.cursor.fract()
            } else {
                0.0
            };
            self.apply(&segments[index], start, 1.0)?;
            if (index + 1) % self.config.checkpoint_every == 0 {
                self.remember(index + 1)?;
            }
        }
        if fraction > 0.0 && completed < segments.len() {
            let start = if forward && completed == baseline {
                self.cursor.fract()
            } else {
                0.0
            };
            self.apply(&segments[completed], start, fraction)?;
        }
        self.cursor = target;
        if rewinding {
            self.dirty_tiles.extend(self.tiles.keys().copied());
        }
        self.trim_snapshots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(shape: CutterShape, spacing_mm: f64) -> SparseConfig {
        SparseConfig {
            stock: StockBox {
                min_mm: [0.0; 3],
                size_mm: [20.0, 20.0, 8.0],
            },
            cutter: CutterSpec {
                shape,
                diameter_mm: 2.0,
                cutting_length_mm: 5.0,
            },
            spacing_mm,
            checkpoint_every: 2,
            max_checkpoints: 3,
            height_budget_mib: 16,
        }
    }
    fn movements() -> [CutSegment; 4] {
        [
            CutSegment {
                from_model_mm: [2.0, 5.0, 10.0],
                to_model_mm: [2.0, 5.0, 4.0],
                spindle_on: true,
                rapid: false,
                nc_line: 1,
            },
            CutSegment {
                from_model_mm: [2.0, 5.0, 4.0],
                to_model_mm: [18.0, 5.0, 4.0],
                spindle_on: true,
                rapid: false,
                nc_line: 2,
            },
            CutSegment {
                from_model_mm: [18.0, 5.0, 4.0],
                to_model_mm: [18.0, 10.0, 10.0],
                spindle_on: true,
                rapid: true,
                nc_line: 3,
            },
            CutSegment {
                from_model_mm: [18.0, 10.0, 10.0],
                to_model_mm: [2.0, 10.0, 10.0],
                spindle_on: false,
                rapid: true,
                nc_line: 4,
            },
        ]
    }
    #[test]
    fn resolution_does_not_change_with_stock_size() {
        let mut small = config(CutterShape::Flat, 0.05);
        small.stock.size_mm[0] = 20.0;
        let a = SparseMaterial::new(small).unwrap();
        small.stock.size_mm[0] = 1000.0;
        let b = SparseMaterial::new(small).unwrap();
        assert!((a.spacing_mm()[0] - 0.05).abs() < 1e-10);
        assert!((b.spacing_mm()[0] - 0.05).abs() < 1e-10);
        assert_eq!(b.stats().tiles, 0);
    }
    #[test]
    fn rollback_and_replay_match_for_both_cutters() {
        for shape in [CutterShape::Flat, CutterShape::Ball] {
            let mut s = SparseMaterial::new(config(shape, 0.1)).unwrap();
            let m = movements();
            s.seek(&m, 4.0).unwrap();
            let initial_final = s.height_at(100, 50).unwrap();
            assert!(initial_final < 8.0);
            s.seek(&m, 0.0).unwrap();
            assert_eq!(s.height_at(100, 50), Some(8.0));
            s.seek(&m, 4.0).unwrap();
            assert_eq!(s.height_at(100, 50).unwrap(), initial_final);
            s.seek(&m, 1.5).unwrap();
            assert!(s.height_at(50, 50).unwrap() < 8.0);
        }
    }
    #[test]
    fn fractional_forward_seek_reuses_current_material_state() {
        for shape in [CutterShape::Flat, CutterShape::Ball] {
            let moves = movements();
            let mut incremental = SparseMaterial::new(config(shape, 0.1)).unwrap();
            let mut direct = SparseMaterial::new(config(shape, 0.1)).unwrap();
            // Comprobamos fracciones consecutivas dentro y entre movimientos.
            for target in [0.1, 0.3, 0.9, 1.0, 1.1, 1.3, 1.9, 2.0, 2.6, 3.0, 4.0] {
                incremental.seek(&moves, target).unwrap();
                direct = SparseMaterial::new(config(shape, 0.1)).unwrap();
                direct.seek(&moves, target).unwrap();

                for y in 0..201 {
                    for x in 0..201 {
                        assert_eq!(
                            incremental.height_at(x, y),
                            direct.height_at(x, y),
                            "Shape {shape:?} seek {target} sample {x},{y}"
                        );
                    }
                }
            }
            incremental.seek(&moves, 0.0).unwrap();
            assert_eq!(incremental.height_at(100, 50), Some(8.0));
        }
    }
    #[test]
    fn material_regions_are_incremental_and_restore_on_rewind() {
        let mut s = SparseMaterial::new(config(CutterShape::Flat, 0.1)).unwrap();
        let moves = movements();
        assert!(s.take_dirty_regions().is_empty());
        s.seek(&moves, 4.0).unwrap();
        let changed = s.take_dirty_regions();
        assert!(!changed.is_empty(), "una pasada debe modificar bloques");
        for region in &changed {
            assert_eq!(region.values.len(), region.width * region.height);
            assert!(region.values.iter().all(|z| z.is_finite()));
            assert!(region.x + region.width <= s.cells()[0]);
            assert!(region.y + region.height <= s.cells()[1]);
        }
        assert!(
            s.take_dirty_regions().is_empty(),
            "sin cambios no hay transferencias"
        );
        s.seek(&moves, 0.0).unwrap();
        let restored = s.take_dirty_regions();
        assert!(
            !restored.is_empty(),
            "rebobinar debe invalidar bloques previamente fresados"
        );
        assert!(restored.iter().all(|r| r.values.iter().all(|v| *v == 8.0)));
        assert!(s.take_dirty_regions().is_empty());
    }

    #[test]
    fn geometric_cut_independent_of_spindle() {
        let mut s = SparseMaterial::new(config(CutterShape::Flat, 0.1)).unwrap();
        let mut moves = movements();
        assert!(!moves.is_empty());
        for segment in &mut moves {
            segment.spindle_on = false;
        }
        s.seek(&moves, moves.len() as f64).unwrap();
        assert!(s.height_at(100, 50).unwrap() < 8.0);
        assert!(!s.take_dirty_regions().is_empty());
    }
    #[test]
    fn advancing_fractionally_matches_independent_seek() {
        for shape in [CutterShape::Flat, CutterShape::Ball] {
            let moves = movements();
            let mut sequential = SparseMaterial::new(config(shape, 0.1)).unwrap();
            for target in [0.10, 0.25, 0.63, 0.92, 1.0, 1.3, 1.9, 2.0, 2.7, 3.3, 4.0] {
                sequential.seek(&moves, target).unwrap();
                let mut direct = SparseMaterial::new(config(shape, 0.1)).unwrap();
                direct.seek(&moves, target).unwrap();
                for y in 0..201 {
                    for x in 0..201 {
                        assert_eq!(
                            sequential.height_at(x, y),
                            direct.height_at(x, y),
                            "Fraccion {target} en {x},{y}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn continuous_swept_cutter_has_analytical_profile_and_rewinds() {
        let line = [CutSegment {
            from_model_mm: [2.0, 10.0, 3.0],
            to_model_mm: [18.0, 10.0, 3.0],
            spindle_on: false,
            rapid: true,
            nc_line: 1,
        }];
        for shape in [CutterShape::Flat, CutterShape::Ball] {
            let mut s = SparseMaterial::new(config(shape, 0.1)).unwrap();
            s.seek(&line, 1.0).unwrap();
            assert!((s.height_at(100, 100).unwrap() - 3.0).abs() < 1e-5);
            let off_center = s.height_at(100, 105).unwrap();
            let expected = match shape {
                CutterShape::Flat => 3.0,
                CutterShape::Ball => 3.0 + 1.0 - (1.0_f64 - 0.5_f64 * 0.5).sqrt(),
            };
            assert!((off_center - expected).abs() < 1e-5);
            assert_eq!(s.height_at(100, 120), Some(8.0));
            s.seek(&line, 0.0).unwrap();
            assert_eq!(s.height_at(100, 100), Some(8.0));
        }
    }
    #[test]
    fn rejects_invalid_config_and_seek() {
        let mut cfg = config(CutterShape::Flat, 0.1);
        cfg.spacing_mm = 0.0;
        assert!(SparseMaterial::new(cfg).is_err());
        let mut s = SparseMaterial::new(config(CutterShape::Flat, 0.1)).unwrap();
        assert!(s.seek(&movements(), 5.0).is_err());
        assert!(s.seek(&movements(), f64::NAN).is_err());
        assert_eq!(s.stats().cursor, 0.0);
    }
}

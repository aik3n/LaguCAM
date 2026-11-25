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
const MAX_STAMPS_PER_MOVE: usize = 8_000_000;
const MAX_CELLS_PER_AXIS: usize = 20_000_001;

type Key = (usize, usize);
type Tile = Arc<[f32]>;
type TileMap = HashMap<Key, Tile>;

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
            snapshots: Vec::new(),
            cursor: 0.0,
            replayed_moves: 0,
        })
    }

    pub fn stock(&self) -> StockBox { self.config.stock }
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

    fn stamp(&mut self, tip: [f64; 3]) {
        let stock = self.config.stock;
        let cutter = self.config.cutter;
        let top_z = stock.min_mm[2] + stock.size_mm[2];
        if tip[2] >= top_z || tip[2] + cutter.cutting_length_mm <= stock.min_mm[2] {
            return;
        }
        let r = cutter.diameter_mm * 0.5;
        if tip[0] + r < stock.min_mm[0]
            || tip[0] - r > stock.min_mm[0] + stock.size_mm[0]
            || tip[1] + r < stock.min_mm[1]
            || tip[1] - r > stock.min_mm[1] + stock.size_mm[1]
        {
            return;
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
        let x0 = index(0, tip[0] - r, false);
        let x1 = index(0, tip[0] + r, true);
        let y0 = index(1, tip[1] - r, false);
        let y1 = index(1, tip[1] + r, true);
        let top = stock.size_mm[2] as f32;
        for iy in y0..=y1 {
            let y = stock.min_mm[1] + iy as f64 * self.spacing[1];
            let dy = y - tip[1];
            for ix in x0..=x1 {
                let x = stock.min_mm[0] + ix as f64 * self.spacing[0];
                let dx = x - tip[0];
                let rr = dx * dx + dy * dy;
                if rr > r * r {
                    continue;
                }
                let bottom = match cutter.shape {
                    CutterShape::Flat => 0.0,
                    CutterShape::Ball => r - (r * r - rr).max(0.0).sqrt(),
                };
                let z = ((tip[2] + bottom - stock.min_mm[2]).clamp(0.0, stock.size_mm[2])) as f32;
                if z >= top {
                    continue;
                }
                let entry = self
                    .tiles
                    .entry((ix / TILE, iy / TILE))
                    .or_insert_with(|| Arc::from(vec![top; SAMPLES_PER_TILE]));
                let p = (iy % TILE) * TILE + ix % TILE;
                if z < entry[p] {
                    Arc::make_mut(entry)[p] = z;
                }
            }
        }
    }

    fn apply(&mut self, segment: &CutSegment, fraction: f64) -> Result<(), String> {
        if !segment.spindle_on {
            return Ok(());
        }
        let stock = self.config.stock;
        let r = self.config.cutter.diameter_mm * 0.5;
        for axis in 0..3 {
            let low = segment.from_model_mm[axis].min(segment.to_model_mm[axis]);
            let high = segment.from_model_mm[axis].max(segment.to_model_mm[axis]);
            let min_stock = if axis == 2 {
                stock.min_mm[2]
            } else {
                stock.min_mm[axis] - r
            };
            let max_stock = if axis == 2 {
                stock.min_mm[2] + stock.size_mm[2]
            } else {
                stock.min_mm[axis] + stock.size_mm[axis] + r
            };
            if high < min_stock || low > max_stock {
                return Ok(());
            }
        }
        let delta: [f64; 3] =
            std::array::from_fn(|axis| segment.to_model_mm[axis] - segment.from_model_mm[axis]);
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(format!(
                "Linea {}: movimiento NC no finito",
                segment.nc_line
            ));
        }
        let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
        let required = (length / (self.spacing[0].min(self.spacing[1]) * 0.5)).ceil();
        if !required.is_finite() || required > MAX_STAMPS_PER_MOVE as f64 {
            return Err(format!(
                "Linea {}: demasiadas muestras de barrido",
                segment.nc_line
            ));
        }
        let count = (required as usize).max(1);
        let last = ((count as f64 * fraction).floor() as usize).min(count);
        for k in 0..=last {
            let t = k as f64 / count as f64;
            self.stamp(std::array::from_fn(|axis| {
                segment.from_model_mm[axis] + delta[axis] * t
            }));
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
        let completed = target.floor() as usize;
        let fraction = target - completed as f64;

        // Partir del estado actual solo cuando avanzamos desde un entero.
        let current_integer = self.cursor.fract() == 0.0;
        let forward = target >= self.cursor && current_integer;
        let baseline = if forward {
            completed.min(self.cursor as usize)
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
            self.apply(&segments[index], 1.0)?;
            if (index + 1) % self.config.checkpoint_every == 0 {
                self.remember(index + 1)?;
            }
        }
        if fraction > 0.0 && completed < segments.len() {
            self.apply(&segments[completed], fraction)?;
        }
        self.cursor = target;
        self.trim_snapshots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_sim::MaterialHeightfield;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

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
    fn agrees_with_existing_engine_on_sample_grid() {
        for shape in [CutterShape::Flat, CutterShape::Ball] {
            let cfg = config(shape, 0.1);
            let seg = movements();
            let mut sparse = SparseMaterial::new(cfg).unwrap();
            let mut original = MaterialHeightfield::simulate_with_budget(
                cfg.stock,
                cfg.cutter,
                201,
                128,
                &seg,
                &AtomicBool::new(false),
                &AtomicUsize::new(0),
            )
            .unwrap();
            for cursor in [0.0, 1.0, 2.0, 4.0, 1.5, 4.0] {
                sparse.seek(&seg, cursor).unwrap();
                original.seek(cursor);
                for y in 0..201 {
                    for x in 0..201 {
                        let diff =
                            (sparse.height_at(x, y).unwrap() - original.height_at(x, y)).abs();
                        assert!(
                            diff <= 1e-5,
                            "{shape:?}: NC {cursor} ({x},{y}), diff {diff}"
                        );
                    }
                }
            }
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

//! P2-D2: simulacion de material para 3 ejes por superficie de alturas.
//! La unica entrada de movimiento es la traza NC interpretada (nunca PlannedToolpath).
//! Una altura Z por XY: no representa socavados, cavidades cerradas ni 5 ejes.
//! El barrido se discretiza sobre el segmento NC; es una aproximacion, no certificacion CNC.

use crate::nc_contract::{NcEventKind, NcMotionKind, NcSpindleMode, NcTrace};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const MAX_SIM_MOVES: usize = 100_000;

const MAX_SWEEP_SAMPLES: usize = 8_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutterShape {
    Flat,
    Ball,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutterSpec {
    pub shape: CutterShape,
    pub diameter_mm: f64,
    pub cutting_length_mm: f64,
}

impl CutterSpec {
    pub fn validate(self) -> Result<(), String> {
        if !self.diameter_mm.is_finite()
            || self.diameter_mm <= 0.0
            || !self.cutting_length_mm.is_finite()
            || self.cutting_length_mm <= 0.0
            || (self.shape == CutterShape::Ball && self.cutting_length_mm < self.diameter_mm * 0.5)
        {
            return Err("Fresa plana o esferica no valida para simulacion".into());
        }
        Ok(())
    }

    /// Altura minima de la superficie cortante sobre la punta para un radio XY.
    fn bottom_at_radius(self, radius_sq: f64) -> Option<f64> {
        let radius = self.diameter_mm * 0.5;
        if radius_sq > radius * radius {
            return None;
        }
        match self.shape {
            CutterShape::Flat => Some(0.0),
            CutterShape::Ball => Some(radius - (radius * radius - radius_sq).max(0.0).sqrt()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockBox {
    pub min_mm: [f64; 3],
    pub size_mm: [f64; 3],
}

impl StockBox {
    pub fn validate(self) -> Result<(), String> {
        if !self.min_mm.iter().all(|x| x.is_finite())
            || !self.size_mm.iter().all(|x| x.is_finite() && *x > 0.0)
            || !(0..3).all(|i| (self.min_mm[i] + self.size_mm[i]).is_finite())
            || self.size_mm[2] > f32::MAX as f64
        {
            return Err("Dimensiones del bruto no validas".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutSegment {
    pub from_model_mm: [f64; 3],
    pub to_model_mm: [f64; 3],
    pub spindle_on: bool,
    pub rapid: bool,
    pub nc_line: usize,
}

/// Preserva el orden de todos los movimientos interpretados, tambien con M5.
/// Ningun G0 se convierte arbitrariamente en movimiento libre de contacto.
pub fn segments_from_trace(
    trace: &NcTrace,
    mut to_model: impl FnMut([f64; 3]) -> Result<[f64; 3], String>,
) -> Result<Vec<CutSegment>, String> {
    trace.validate().map_err(|error| error.to_string())?;
    let mut spindle_on = trace.initial.spindle_mode != NcSpindleMode::Stopped;
    let mut segments = Vec::new();
    for event in &trace.events {
        match event.kind {
            NcEventKind::Spindle(mode) => spindle_on = mode != NcSpindleMode::Stopped,
            NcEventKind::SelectTool(_) | NcEventKind::ToolChange(_) => {
                return Err(format!(
                    "Linea {}: cambio de herramienta no soportado en simulacion mono-herramienta",
                    event.source_line
                ));
            }
            NcEventKind::Motion(motion) => {
                if segments.len() >= MAX_SIM_MOVES {
                    return Err(
                        "Programa NC demasiado largo para el prototipo de simulacion".into(),
                    );
                }
                let from_model_mm = to_model(motion.from_work_mm)?;
                let to_model_mm = to_model(motion.to_work_mm)?;
                if !from_model_mm
                    .iter()
                    .chain(to_model_mm.iter())
                    .all(|v| v.is_finite())
                {
                    return Err(format!(
                        "Linea {}: coordenadas no finitas",
                        event.source_line
                    ));
                }
                segments.push(CutSegment {
                    from_model_mm,
                    to_model_mm,
                    spindle_on,
                    rapid: motion.kind == NcMotionKind::Rapid,
                    nc_line: event.source_line,
                });
            }
            _ => {}
        }
    }
    Ok(segments)
}

/// Cambio cronologico de una muestra. El historial guarda valores anteriores
/// para el rebobinado exacto respecto de ESTA discretizacion, no de la CNC.
#[derive(Clone, Copy, Debug)]
struct HeightChange {
    at: f64,
    index: u32,
    before: f32,
    after: f32,
}

/// Historial cronologico con bloques pequenos: la RAM se consume por demanda.
/// Nunca guarda copias completas del bruto por movimiento.
const HISTORY_CHUNK_SIZE: usize = 4096;
const MIB: usize = 1024 * 1024;
const DEFAULT_HISTORY_BUDGET_MIB: usize = 256;

#[derive(Debug)]
struct ChangeLog {
    blocks: Vec<Vec<HeightChange>>,
    len: usize,
    max_entries: usize,
    budget_bytes: usize,
}

impl ChangeLog {
    fn new(budget_mib: usize) -> Result<Self, String> {
        if !matches!(budget_mib, 128 | 256 | 512) {
            return Err("Presupuesto de RAM no valido: elige 128, 256 o 512 MiB".into());
        }
        let budget_bytes = budget_mib
            .checked_mul(MIB)
            .ok_or("Presupuesto de RAM fuera de rango")?;
        Ok(Self {
            blocks: Vec::new(),
            len: 0,
            max_entries: budget_bytes / std::mem::size_of::<HeightChange>(),
            budget_bytes,
        })
    }

    fn push(&mut self, change: HeightChange) -> Result<(), String> {
        if self.len >= self.max_entries {
            return Err(
                "Presupuesto RAM del historial agotado. Aumenta el limite o baja resolucion".into(),
            );
        }
        if self.len % HISTORY_CHUNK_SIZE == 0 {
            self.blocks
                .try_reserve(1)
                .map_err(|_| "No se pudo ampliar el indice del historial")?;
            let mut block = Vec::new();
            let capacity = (self.max_entries - self.len).min(HISTORY_CHUNK_SIZE);
            block
                .try_reserve_exact(capacity)
                .map_err(|_| "Sin RAM suficiente para un bloque del historial")?;
            self.blocks.push(block);
        }
        self.blocks
            .last_mut()
            .ok_or("Bloque de historial ausente")?
            .push(change);
        self.len += 1;
        Ok(())
    }

    fn get(&self, index: usize) -> HeightChange {
        self.blocks[index / HISTORY_CHUNK_SIZE][index % HISTORY_CHUNK_SIZE]
    }

    fn allocated_bytes(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| block.capacity() * std::mem::size_of::<HeightChange>())
            .sum()
    }
}

#[derive(Debug)]
pub struct MaterialHeightfield {
    pub stock: StockBox,
    pub cells: [usize; 2],
    /// Alturas respecto a Z minima del bruto: preserva precision con STL desplazados.
    heights: Vec<f32>,
    history: ChangeLog,
    cursor: usize,
    pub total_moves: usize,
    pub rapid_cut_moves: usize,
    removed_mm3: f64,
    top_rel: f32,
}

impl MaterialHeightfield {
    /// Mantener la API anterior para tests y clientes que usan el limite por defecto.
    pub fn simulate(
        stock: StockBox,
        cutter: CutterSpec,
        resolution: usize,
        segments: &[CutSegment],
        cancelled: &AtomicBool,
        completed_moves: &AtomicUsize,
    ) -> Result<Self, String> {
        Self::simulate_with_budget(
            stock,
            cutter,
            resolution,
            DEFAULT_HISTORY_BUDGET_MIB,
            segments,
            cancelled,
            completed_moves,
        )
    }

    pub fn simulate_with_budget(
        stock: StockBox,
        cutter: CutterSpec,
        resolution: usize,
        budget_mib: usize,
        segments: &[CutSegment],
        cancelled: &AtomicBool,
        completed_moves: &AtomicUsize,
    ) -> Result<Self, String> {
        stock.validate()?;
        cutter.validate()?;
        if !(128..=1024).contains(&resolution) || segments.len() > MAX_SIM_MOVES {
            return Err("Resolucion fuera de 128..1024 o demasiados movimientos NC".into());
        }
        let side = stock.size_mm[0].max(stock.size_mm[1]);
        let cells: [usize; 2] = std::array::from_fn(|i| {
            ((stock.size_mm[i] / side * resolution as f64).ceil() as usize).clamp(2, resolution)
        });
        let count = cells[0]
            .checked_mul(cells[1])
            .ok_or("Demasiadas muestras XY")?;
        let top_rel = stock.size_mm[2] as f32;
        let mut heights = vec![top_rel; count];
        let mut history = ChangeLog::new(budget_mib)?;
        let mut rapid_cut_moves = 0;
        let dx = stock.size_mm[0] / (cells[0] - 1) as f64;
        let dy = stock.size_mm[1] / (cells[1] - 1) as f64;
        let step = dx.min(dy) * 0.5;
        let mut sweep_samples = 0usize;
        completed_moves.store(0, Ordering::Relaxed);
        for (move_index, segment) in segments.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Simulacion cancelada".into());
            }
            let delta: [f64; 3] =
                std::array::from_fn(|axis| segment.to_model_mm[axis] - segment.from_model_mm[axis]);
            if !delta.iter().all(|v| v.is_finite()) {
                return Err(format!(
                    "Linea {}: movimiento fuera del rango numerico",
                    segment.nc_line
                ));
            }
            // El paso incluye Z: las entradas verticales y rampas no se omiten.
            let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            if !length.is_finite() {
                return Err("Longitud NC fuera de rango".into());
            }
            if !segment.spindle_on || !Self::intersects_stock(stock, cutter, segment) {
                completed_moves.store(move_index + 1, Ordering::Relaxed);
                continue;
            }
            let required = (length / step).ceil();
            if !required.is_finite() || required > MAX_SWEEP_SAMPLES as f64 {
                return Err(format!(
                    "Linea {}: barrido demasiado largo para la resolucion",
                    segment.nc_line
                ));
            }
            let steps = (required as usize).max(1);
            sweep_samples = sweep_samples
                .checked_add(steps + 1)
                .ok_or("Muestras desbordadas")?;
            if sweep_samples > MAX_SWEEP_SAMPLES {
                return Err(
                    "Presupuesto de barrido superado; baja la resolucion o divide el programa"
                        .into(),
                );
            }
            let mut rapid_removed = false;
            for s in 0..=steps {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Simulacion cancelada".into());
                }
                let fraction = s as f64 / steps as f64;
                let tip: [f64; 3] = std::array::from_fn(|axis| {
                    segment.from_model_mm[axis] + delta[axis] * fraction
                });
                let stamp = move_index as f64 + fraction.max(1.0e-9);
                let changed =
                    Self::apply_tip(&mut heights, &mut history, stock, cells, cutter, tip, stamp)?;
                rapid_removed |= changed;
            }
            if segment.rapid && rapid_removed {
                rapid_cut_moves += 1;
            }
            completed_moves.store(move_index + 1, Ordering::Relaxed);
        }
        Ok(Self {
            stock,
            cells,
            heights: vec![top_rel; count],
            history,
            cursor: 0,
            total_moves: segments.len(),
            rapid_cut_moves,
            removed_mm3: 0.0,
            top_rel,
        })
    }

    fn intersects_stock(stock: StockBox, cutter: CutterSpec, seg: &CutSegment) -> bool {
        let radius = cutter.diameter_mm * 0.5;
        (0..3).all(|axis| {
            let below = if axis == 2 { 0.0 } else { radius };
            let above = if axis == 2 {
                cutter.cutting_length_mm
            } else {
                radius
            };
            let low = seg.from_model_mm[axis].min(seg.to_model_mm[axis]) - below;
            let high = seg.from_model_mm[axis].max(seg.to_model_mm[axis]) + above;
            low <= stock.min_mm[axis] + stock.size_mm[axis] && high >= stock.min_mm[axis]
        })
    }

    fn apply_tip(
        heights: &mut [f32],
        history: &mut ChangeLog,
        stock: StockBox,
        cells: [usize; 2],
        cutter: CutterSpec,
        tip: [f64; 3],
        at: f64,
    ) -> Result<bool, String> {
        let radius = cutter.diameter_mm * 0.5;
        if tip[2] >= stock.min_mm[2] + stock.size_mm[2]
            || tip[2] + cutter.cutting_length_mm <= stock.min_mm[2]
        {
            return Ok(false);
        }
        let [nx, ny] = cells;
        let spacing = [
            stock.size_mm[0] / (nx - 1) as f64,
            stock.size_mm[1] / (ny - 1) as f64,
        ];
        let low = [tip[0] - radius, tip[1] - radius];
        let high = [tip[0] + radius, tip[1] + radius];
        if (0..2).any(|axis| {
            high[axis] < stock.min_mm[axis] || low[axis] > stock.min_mm[axis] + stock.size_mm[axis]
        }) {
            return Ok(false);
        }
        let bounds = [nx, ny];
        let lo: [usize; 2] = std::array::from_fn(|axis| {
            (((low[axis] - stock.min_mm[axis]) / spacing[axis]).floor() as isize)
                .clamp(0, bounds[axis] as isize - 1) as usize
        });
        let hi: [usize; 2] = std::array::from_fn(|axis| {
            (((high[axis] - stock.min_mm[axis]) / spacing[axis]).ceil() as isize)
                .clamp(0, bounds[axis] as isize - 1) as usize
        });
        let mut changed = false;
        for iy in lo[1]..=hi[1] {
            let y = stock.min_mm[1] + iy as f64 * spacing[1];
            let dy = y - tip[1];
            for ix in lo[0]..=hi[0] {
                let x = stock.min_mm[0] + ix as f64 * spacing[0];
                let dx = x - tip[0];
                let Some(bottom) = cutter.bottom_at_radius(dx * dx + dy * dy) else {
                    continue;
                };
                let new_rel =
                    ((tip[2] + bottom - stock.min_mm[2]).clamp(0.0, stock.size_mm[2])) as f32;
                let index = iy * nx + ix;
                let old_rel = heights[index];
                if new_rel < old_rel {
                    history.push(HeightChange {
                        at,
                        index: index as u32,
                        before: old_rel,
                        after: new_rel,
                    })?;
                    heights[index] = new_rel;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    /// Avance y retroceso sobre el mismo historial ordenado por G-code interpretado.
    pub fn seek(&mut self, playhead: f64) {
        let target = if playhead.is_finite() {
            playhead.clamp(0.0, self.total_moves as f64)
        } else {
            0.0
        };
        let cell_area = self.stock.size_mm[0] * self.stock.size_mm[1]
            / ((self.cells[0] - 1) * (self.cells[1] - 1)) as f64;
        while self.cursor < self.history.len && self.history.get(self.cursor).at <= target {
            let event = self.history.get(self.cursor);
            self.heights[event.index as usize] = event.after;
            self.removed_mm3 += (event.before - event.after) as f64 * cell_area;
            self.cursor += 1;
        }
        while self.cursor > 0 && self.history.get(self.cursor - 1).at > target {
            self.cursor -= 1;
            let event = self.history.get(self.cursor);
            self.heights[event.index as usize] = event.before;
            self.removed_mm3 -= (event.before - event.after) as f64 * cell_area;
        }
        if self.cursor == 0 {
            self.removed_mm3 = 0.0;
        }
    }

    pub fn height_at(&self, ix: usize, iy: usize) -> f64 {
        self.stock.min_mm[2] + self.heights[iy * self.cells[0] + ix] as f64
    }

    pub fn is_cut(&self, ix: usize, iy: usize) -> bool {
        self.heights[iy * self.cells[0] + ix] < self.top_rel
    }

    pub fn removed_mm3(&self) -> f64 {
        self.removed_mm3.max(0.0)
    }
    pub fn history_len(&self) -> usize {
        self.history.len
    }
    pub fn history_memory_bytes(&self) -> usize {
        self.history.allocated_bytes()
    }
    pub fn history_budget_mib(&self) -> usize {
        self.history.budget_bytes / MIB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        nc_contract::{NcDistanceMode, NcInitialState, NcUnits},
        nc_interpreter::interpret_program,
    };
    fn stock() -> StockBox {
        StockBox {
            min_mm: [0.0, 0.0, 0.0],
            size_mm: [10.0, 10.0, 5.0],
        }
    }
    fn tool(shape: CutterShape) -> CutterSpec {
        CutterSpec {
            shape,
            diameter_mm: 3.0,
            cutting_length_mm: 4.0,
        }
    }
    fn cut() -> [CutSegment; 1] {
        [CutSegment {
            from_model_mm: [2.0, 5.0, 2.0],
            to_model_mm: [8.0, 5.0, 2.0],
            spindle_on: true,
            rapid: false,
            nc_line: 8,
        }]
    }
    fn run(shape: CutterShape) -> MaterialHeightfield {
        MaterialHeightfield::simulate(
            stock(),
            tool(shape),
            128,
            &cut(),
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap()
    }
    #[test]
    fn change_log_uses_bounded_chunks_and_keeps_event_order() {
        let mut log = ChangeLog::new(256).unwrap();
        for i in 0..(HISTORY_CHUNK_SIZE + 3) {
            log.push(HeightChange {
                at: i as f64,
                index: i as u32,
                before: 5.0,
                after: 2.0,
            })
            .unwrap();
        }
        assert_eq!(log.blocks.len(), 2);
        assert_eq!(log.len, HISTORY_CHUNK_SIZE + 3);
        assert_eq!(
            log.get(HISTORY_CHUNK_SIZE + 2).index,
            (HISTORY_CHUNK_SIZE + 2) as u32
        );
        assert_eq!(
            log.allocated_bytes(),
            2 * HISTORY_CHUNK_SIZE * std::mem::size_of::<HeightChange>()
        );
        assert!(ChangeLog::new(64).is_err());
        assert!(ChangeLog::new(1024).is_err());
    }
    #[test]
    fn flat_cut_produces_a_visible_continuous_channel() {
        let mut field = run(CutterShape::Flat);
        assert!(field.history_len() > 0);
        assert_eq!(field.height_at(64, 64), 5.0);
        field.seek(1.0);
        assert!(field.height_at(64, 64) <= 2.0 + 1.0e-3);
        assert!(field.removed_mm3() > 0.0);
        field.seek(0.0);
        assert_eq!(field.height_at(64, 64), 5.0);
        assert_eq!(field.removed_mm3(), 0.0);
    }
    #[test]
    fn ball_has_rounder_profile_than_flat() {
        let mut flat = run(CutterShape::Flat);
        let mut ball = run(CutterShape::Ball);
        flat.seek(1.0);
        ball.seek(1.0);
        // El centro del barrido es 2 mm; lejos del eje, la esfera quita menos.
        assert!(flat.height_at(64, 76) < ball.height_at(64, 76));
        assert!(flat.removed_mm3() > ball.removed_mm3());
    }
    #[test]
    fn cut_is_continuous_along_each_nc_segment_and_rewinds() {
        let mut field = run(CutterShape::Flat);
        field.seek(0.25);
        assert!(field.height_at(35, 64) < 5.0);
        assert_eq!(field.height_at(110, 64), 5.0);
        field.seek(1.0);
        assert!(field.height_at(96, 64) < 5.0);
        field.seek(0.0);
        assert!(!field.is_cut(64, 64));
    }
    #[test]
    fn spindle_off_does_not_fake_cutting_and_g0_rapid_removal_is_flagged() {
        let mut off = cut();
        off[0].spindle_on = false;
        let v = MaterialHeightfield::simulate(
            stock(),
            tool(CutterShape::Flat),
            128,
            &off,
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert_eq!(v.history_len(), 0);
        let mut fast = cut();
        fast[0].rapid = true;
        let v = MaterialHeightfield::simulate(
            stock(),
            tool(CutterShape::Flat),
            128,
            &fast,
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert_eq!(v.rapid_cut_moves, 1);
    }
    #[test]
    fn coordinates_with_large_offsets_preserve_surface_detail() {
        let s = StockBox {
            min_mm: [1.0e9, 1.0e9, 1.0e9],
            size_mm: [10.0, 10.0, 5.0],
        };
        let seg = [CutSegment {
            from_model_mm: [1.0e9 + 2.0, 1.0e9 + 5.0, 1.0e9 + 2.0],
            to_model_mm: [1.0e9 + 8.0, 1.0e9 + 5.0, 1.0e9 + 2.0],
            spindle_on: true,
            rapid: false,
            nc_line: 2,
        }];
        let mut result = MaterialHeightfield::simulate(
            s,
            tool(CutterShape::Flat),
            128,
            &seg,
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap();
        result.seek(1.0);
        assert!((result.height_at(64, 64) - (1.0e9 + 2.0)).abs() < 0.01);
    }
    #[test]
    fn nc_interpreter_is_the_source_of_segments() {
        let text = "G21\nG90\nG94\nS8000 M3\nF600 G1 X8 Y5 Z2\nM5\nM30\n";
        let initial = NcInitialState {
            position_work_mm: [2.0, 5.0, 2.0],
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        };
        let trace = interpret_program(text, initial).unwrap();
        let segs = segments_from_trace(&trace, Ok).unwrap();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].spindle_on);
        assert_eq!(segs[0].nc_line, 5);
    }
    #[test]
    fn invalid_resolution_and_cancelled_job_are_rejected() {
        let cancelled = AtomicBool::new(true);
        assert!(
            MaterialHeightfield::simulate(
                stock(),
                tool(CutterShape::Flat),
                64,
                &cut(),
                &AtomicBool::new(false),
                &AtomicUsize::new(0)
            )
            .is_err()
        );
        assert!(
            MaterialHeightfield::simulate(
                stock(),
                tool(CutterShape::Flat),
                256,
                &cut(),
                &cancelled,
                &AtomicUsize::new(0)
            )
            .is_err()
        );
    }
}

//! Simulacion experimental Tri-Dexel: intervalos de material a lo largo de X, Y y Z.
//! No deriva una altura Z: admite varios intervalos por rayo y, por tanto, huecos
//! internos y paredes verticales. Primera version: resultado FINAL, herramienta
//! vertical plana o esferica, barrido con muestreo espacial explicito.
use crate::material_sim::{CutSegment, CutterShape, CutterSpec, StockBox};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const MAX_RAYS: usize = 1_600_000;
const MAX_QUADS: usize = 1_600_000;
static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct VisualFrame {
    pub mm_origin: [f64; 3],
    pub visual_origin: [f32; 3],
    /// Desplazamiento visual de 1 mm en cada eje de modelo.
    pub axes: [[f32; 3]; 3],
}
impl VisualFrame {
    fn at(&self, p: [f64; 3]) -> [f32; 3] {
        let delta = [
            (p[0] - self.mm_origin[0]) as f32,
            (p[1] - self.mm_origin[1]) as f32,
            (p[2] - self.mm_origin[2]) as f32,
        ];
        std::array::from_fn(|i| {
            self.visual_origin[i]
                + delta[0] * self.axes[0][i]
                + delta[1] * self.axes[1][i]
                + delta[2] * self.axes[2][i]
        })
    }
}

pub struct TriMesh {
    pub id: u64,
    /// Coordenadas visuales XYZ + RGB por vertice. Sin indices compartidos.
    pub vertices: Vec<f32>,
    pub quads: usize,
}

pub struct TriDexel {
    stock: StockBox,
    cutter: CutterSpec,
    cells: [usize; 3],
    step: [f64; 3],
    /// Almacenamiento disperso. Ausencia = intervalo del bruto intacto.
    /// Vec vacio = rayo completamente eliminado.
    rays: [HashMap<usize, Vec<Interval>>; 3],
}

impl TriDexel {
    pub fn new(stock: StockBox, cutter: CutterSpec, spacing_mm: f64) -> Result<Self, String> {
        stock.validate()?;
        cutter.validate()?;
        if !spacing_mm.is_finite() || spacing_mm <= 0.0 {
            return Err("Paso Tri-Dexel invalido".into());
        }
        let mut cells = [0usize; 3];
        let mut step = [0.0; 3];
        for axis in 0..3 {
            let intervals = (stock.size_mm[axis] / spacing_mm).ceil();
            if !intervals.is_finite() || intervals < 1.0 || intervals > 100_000.0 {
                return Err("Dimensiones Tri-Dexel fuera de rango".into());
            }
            cells[axis] = intervals as usize + 1;
            step[axis] = stock.size_mm[axis] / intervals;
        }
        let rays = cells[0]
            .checked_mul(cells[1])
            .and_then(|n| n.checked_add(cells[0].checked_mul(cells[2])?))
            .and_then(|n| n.checked_add(cells[1].checked_mul(cells[2])?))
            .ok_or("Desbordamiento de rayos Tri-Dexel")?;
        if rays > MAX_RAYS {
            return Err(format!(
                "Tri-Dexel experimental requiere {rays} rayos (maximo {MAX_RAYS}). Ajusta explicitamente su paso o utiliza un bruto mas pequeno; no se reduce la resolucion automaticamente."
            ));
        }
        Ok(Self {
            stock,
            cutter,
            cells,
            step,
            rays: std::array::from_fn(|_| HashMap::new()),
        })
    }

    pub fn cells(&self) -> [usize; 3] {
        self.cells
    }
    pub fn changed_rays(&self) -> usize {
        self.rays.iter().map(HashMap::len).sum()
    }

    fn coordinate(&self, axis: usize, ix: usize) -> f64 {
        self.stock.min_mm[axis] + self.step[axis] * ix as f64
    }

    fn index_range(&self, axis: usize, low: f64, high: f64) -> Option<(usize, usize)> {
        let min = self.stock.min_mm[axis];
        let max = min + self.stock.size_mm[axis];
        if high < min || low > max {
            return None;
        }
        let start = (((low - min) / self.step[axis]).ceil() as isize)
            .clamp(0, (self.cells[axis] - 1) as isize) as usize;
        let end = (((high - min) / self.step[axis]).floor() as isize)
            .clamp(0, (self.cells[axis] - 1) as isize) as usize;
        if start > end {
            None
        } else {
            Some((start, end))
        }
    }

    fn full_interval(&self, axis: usize) -> Interval {
        Interval {
            start: self.stock.min_mm[axis] as f32,
            end: (self.stock.min_mm[axis] + self.stock.size_mm[axis]) as f32,
        }
    }

    fn ray_key(&self, axis: usize, i: usize, j: usize) -> usize {
        match axis {
            0 => j * self.cells[1] + i, // i=Y, j=Z
            1 => j * self.cells[0] + i, // i=X, j=Z
            _ => j * self.cells[0] + i, // i=X, j=Y
        }
    }

    fn carve(&mut self, axis: usize, key: usize, lo: f64, hi: f64) {
        let full = self.full_interval(axis);
        let lo = (lo as f32).max(full.start);
        let hi = (hi as f32).min(full.end);
        if !(lo < hi) {
            return;
        }
        let intervals = self.rays[axis].entry(key).or_insert_with(|| vec![full]);
        let mut remaining = Vec::with_capacity(intervals.len() + 1);
        for old in intervals.iter() {
            if old.end <= lo || old.start >= hi {
                remaining.push(*old);
            } else {
                if old.start < lo {
                    remaining.push(Interval {
                        start: old.start,
                        end: lo,
                    });
                }
                if old.end > hi {
                    remaining.push(Interval {
                        start: hi,
                        end: old.end,
                    });
                }
            }
        }
        *intervals = remaining;
    }

    /// Resta el volumen de una fresa VERTICAL situada en `tip`.
    /// El estado del husillo y G0/G1 no intervienen en la geometria.
    pub fn carve_tip(&mut self, tip: [f64; 3]) {
        let r = self.cutter.diameter_mm * 0.5;
        let r2 = r * r;
        let z_low = tip[2];
        let z_high = tip[2] + self.cutter.cutting_length_mm;
        if z_low >= self.stock.min_mm[2] + self.stock.size_mm[2] || z_high <= self.stock.min_mm[2] {
            return;
        }
        let Some((x0, x1)) = self.index_range(0, tip[0] - r, tip[0] + r) else {
            return;
        };
        let Some((y0, y1)) = self.index_range(1, tip[1] - r, tip[1] + r) else {
            return;
        };
        let Some((z0, z1)) = self.index_range(2, z_low, z_high) else {
            return;
        };

        // Rayos Z: se resta un intervalo sobre una seccion circular o hemisferica.
        for iy in y0..=y1 {
            let dy = self.coordinate(1, iy) - tip[1];
            for ix in x0..=x1 {
                let dx = self.coordinate(0, ix) - tip[0];
                let rr = dx * dx + dy * dy;
                if rr >= r2 {
                    continue;
                }
                let bottom = match self.cutter.shape {
                    CutterShape::Flat => 0.0,
                    CutterShape::Ball => r - (r2 - rr).sqrt(),
                };
                let key = self.ray_key(2, ix, iy);
                self.carve(2, key, z_low + bottom, z_high);
            }
        }
        // Rayos X,Y: sus extremos describen paredes y huecos internos.
        for iz in z0..=z1 {
            let z = self.coordinate(2, iz);
            let radius2 = match self.cutter.shape {
                CutterShape::Flat => r2,
                CutterShape::Ball if z < tip[2] + r => {
                    let offset = z - (tip[2] + r);
                    (r2 - offset * offset).max(0.0)
                }
                CutterShape::Ball => r2,
            };
            if radius2 <= 0.0 {
                continue;
            }
            for iy in y0..=y1 {
                let dy = self.coordinate(1, iy) - tip[1];
                if dy * dy >= radius2 {
                    continue;
                }
                let dx = (radius2 - dy * dy).sqrt();
                let key = self.ray_key(0, iy, iz);
                self.carve(0, key, tip[0] - dx, tip[0] + dx);
            }
            for ix in x0..=x1 {
                let dx = self.coordinate(0, ix) - tip[0];
                if dx * dx >= radius2 {
                    continue;
                }
                let dy = (radius2 - dx * dx).sqrt();
                let key = self.ray_key(1, ix, iz);
                self.carve(1, key, tip[1] - dy, tip[1] + dy);
            }
        }
    }

    pub fn simulate_final(
        &mut self,
        segments: &[CutSegment],
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let step = self.step.iter().copied().fold(f64::INFINITY, f64::min) * 0.5;
        for segment in segments {
            if cancel.load(Ordering::Relaxed) {
                return Err("Tri-Dexel cancelado".into());
            }
            let delta: [f64; 3] =
                std::array::from_fn(|i| segment.to_model_mm[i] - segment.from_model_mm[i]);
            if !delta.iter().all(|v| v.is_finite()) {
                return Err("NC no finito".into());
            }
            let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            let samples = (length / step).ceil().max(1.0);
            if samples > 2_000_000.0 {
                return Err("Movimiento NC demasiado largo para Tri-Dexel experimental".into());
            }
            for sample in 0..=(samples as usize) {
                if (sample & 63) == 0 && cancel.load(Ordering::Relaxed) {
                    return Err("Tri-Dexel cancelado".into());
                }
                let t = sample as f64 / samples;
                self.carve_tip(std::array::from_fn(|axis| {
                    segment.from_model_mm[axis] + delta[axis] * t
                }));
            }
        }
        Ok(())
    }

    /// Representacion inicial: parches en los extremos de los tres conjuntos
    /// de rayos. Es una visualizacion Tri-Dexel, NO una malla manifold certificada.
    pub fn mesh(&self, frame: VisualFrame) -> Result<TriMesh, String> {
        let total_rays = self.cells[0] * self.cells[1]
            + self.cells[0] * self.cells[2]
            + self.cells[1] * self.cells[2];
        if total_rays.saturating_mul(2) > MAX_QUADS {
            return Err(format!(
                "La vista Tri-Dexel necesita mas de {MAX_QUADS} parches; reduce explicitamente su densidad"
            ));
        }
        let mut vertices = Vec::with_capacity(total_rays * 2 * 6 * 6);
        let mut quads = 0usize;
        for axis in 0..3 {
            let (u, v) = match axis {
                0 => (1, 2),
                1 => (0, 2),
                _ => (0, 1),
            };
            for j in 0..self.cells[v] {
                for i in 0..self.cells[u] {
                    let key = self.ray_key(axis, i, j);
                    let full = [self.full_interval(axis)];
                    let intervals = self.rays[axis]
                        .get(&key)
                        .map(Vec::as_slice)
                        .unwrap_or(&full);
                    let a = self.coordinate(u, i);
                    let b = self.coordinate(v, j);
                    let u0 = (a - self.step[u] * 0.5).max(self.stock.min_mm[u]);
                    let u1 =
                        (a + self.step[u] * 0.5).min(self.stock.min_mm[u] + self.stock.size_mm[u]);
                    let v0 = (b - self.step[v] * 0.5).max(self.stock.min_mm[v]);
                    let v1 =
                        (b + self.step[v] * 0.5).min(self.stock.min_mm[v] + self.stock.size_mm[v]);
                    if u0 >= u1 || v0 >= v1 {
                        continue;
                    }
                    for interval in intervals {
                        for (pos, normal) in
                            [(interval.start as f64, -1.0), (interval.end as f64, 1.0)]
                        {
                            let point = |uu: f64, vv: f64| -> [f64; 3] {
                                let mut p = [0.0; 3];
                                p[axis] = pos;
                                p[u] = uu;
                                p[v] = vv;
                                p
                            };
                            let coords =
                                [point(u0, v0), point(u1, v0), point(u1, v1), point(u0, v1)];
                            let is_stock = (pos - self.stock.min_mm[axis]).abs() < 0.000001
                                || (pos - self.stock.min_mm[axis] - self.stock.size_mm[axis]).abs()
                                    < 0.000001;
                            let color = if is_stock {
                                [0.72, 0.51, 0.30]
                            } else {
                                [0.20, 0.65, 0.83]
                            };
                            let winding = if (axis == 1) == (normal > 0.0) {
                                [0, 2, 1, 0, 3, 2]
                            } else {
                                [0, 1, 2, 0, 2, 3]
                            };
                            for vertex in winding {
                                vertices.extend_from_slice(&frame.at(coords[vertex]));
                                vertices.extend_from_slice(&color);
                            }
                            quads += 1;
                        }
                    }
                }
            }
        }
        if vertices.len() / 6 > i32::MAX as usize {
            return Err("Tri-Dexel supera el limite de vertices OpenGL".into());
        }
        Ok(TriMesh {
            id: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed),
            vertices,
            quads,
        })
    }

    pub fn ray_intervals(&self, axis: usize, i: usize, j: usize) -> Option<Vec<Interval>> {
        if axis >= 3 {
            return None;
        }
        let (u, v) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        if i >= self.cells[u] || j >= self.cells[v] {
            return None;
        }
        let key = self.ray_key(axis, i, j);
        Some(
            self.rays[axis]
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vec![self.full_interval(axis)]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stock() -> StockBox {
        StockBox {
            min_mm: [0.0; 3],
            size_mm: [8.0, 8.0, 8.0],
        }
    }
    fn cutter() -> CutterSpec {
        CutterSpec {
            shape: CutterShape::Flat,
            diameter_mm: 2.0,
            cutting_length_mm: 2.0,
        }
    }
    #[test]
    fn tunnel_is_representable_by_two_z_intervals() {
        let mut d = TriDexel::new(stock(), cutter(), 0.5).unwrap();
        d.carve_tip([4.0, 4.0, 2.0]);
        let intervals = d.ray_intervals(2, 8, 8).unwrap();
        assert_eq!(intervals.len(), 2);
        assert_eq!(
            intervals[0],
            Interval {
                start: 0.0,
                end: 2.0
            }
        );
        assert_eq!(
            intervals[1],
            Interval {
                start: 4.0,
                end: 8.0
            }
        );
        // El volumen con dos intervalos por XY no cabe en un heightfield Z(x,y).
    }
    #[test]
    fn independent_of_spindle_and_rapid() {
        let mut d = TriDexel::new(stock(), cutter(), 0.5).unwrap();
        let segment = CutSegment {
            from_model_mm: [4.0, 4.0, 2.0],
            to_model_mm: [4.0, 4.0, 2.0],
            spindle_on: false,
            rapid: true,
            nc_line: 1,
        };
        d.simulate_final(&[segment], &AtomicBool::new(false))
            .unwrap();
        assert_eq!(d.ray_intervals(2, 8, 8).unwrap().len(), 2);
    }
    #[test]
    fn mesh_has_faces_in_three_directions() {
        let mut d = TriDexel::new(stock(), cutter(), 1.0).unwrap();
        d.carve_tip([4.0, 4.0, 2.0]);
        let frame = VisualFrame {
            mm_origin: [0.0; 3],
            visual_origin: [0.0; 3],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let mesh = d.mesh(frame).unwrap();
        assert!(mesh.quads > 0);
        assert_eq!(mesh.vertices.len(), mesh.quads * 36);
    }
    #[test]
    fn stops_when_stock_exceeds_budget_instead_of_decimating() {
        let huge = StockBox {
            min_mm: [0.0; 3],
            size_mm: [100.0, 100.0, 20.0],
        };
        assert!(TriDexel::new(huge, cutter(), 0.05).is_err());
    }
}

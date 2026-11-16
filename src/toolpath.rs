//! G25.1: recorrido geométrico independiente de egui, OpenGL y G-code.
//!
//! Cada segmento describe únicamente contactos consecutivos de corte.
//! La ausencia de contacto parte el segmento. Nunca se infieren enlaces,
//! rápidos, aproximaciones, retracciones ni movimientos seguros de máquina.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Único movimiento permitido en G25.1: candidato geométrico de corte.
/// No equivale a una trayectoria mecanizable sin verificación adicional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    Cutting,
}

/// Orden del barrido (solo corte). No produce enlaces ni movimientos seguros.
/// El valor predeterminado preserva el sentido de G25.1 y su JSON anterior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RasterPattern {
    #[default]
    Unidirectional,
    Zigzag,
}

impl RasterPattern {
    pub fn is_unidirectional(&self) -> bool {
        matches!(self, Self::Unidirectional)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unidirectional => "Unidireccional",
            Self::Zigzag => "Zigzag (bidireccional)",
        }
    }
}

/// Secuencia contigua en una misma pasada de muestreo.
/// `first_sample` guarda el indice en la rejilla de la PRIMERA muestra
/// visitada. En pasadas zigzag impares puede ser el índice mayor.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionSegment {
    pub kind: MotionKind,
    pub pass_index: usize,
    pub first_sample: usize,
    pub points: Vec<[f64; 3]>,
}

/// Resultado CAM en memoria. No se serializa en el proyecto `.lagucam`.
/// Los segmentos se ordenan por pasada y después según el sentido de corte;
/// una pasada inversa mantiene índices de muestra originales.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub segments: Vec<MotionSegment>,
    pub pattern: RasterPattern,
    pub raster_passes: usize,
    pub samples_per_pass: usize,
    pub contact_points: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolpathError {
    EmptyRaster,
    NonRectangularRaster,
    NonFinitePoint,
    SampleLimitExceeded,
    ArithmeticRange,
}

impl fmt::Display for ToolpathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRaster => f.write_str("Muestreo CAM vacio"),
            Self::NonRectangularRaster => f.write_str("Muestreo CAM no rectangular"),
            Self::NonFinitePoint => f.write_str("Punto CAM con coordenada no finita"),
            Self::SampleLimitExceeded => f.write_str("Muestreo CAM excede el limite de puntos"),
            Self::ArithmeticRange => f.write_str("Contadores CAM fuera de rango"),
        }
    }
}

impl std::error::Error for ToolpathError {}

impl Toolpath {
    /// Consume la rejilla histórica TEST, sin alterar sus posiciones `f64`.
    /// Un `None` separa tramos y el final de una pasada nunca se une a otra.
    /// La capacidad se valida ANTES de producir segmentos.
    pub fn from_contact_grid(
        rows: Vec<Vec<Option<[f64; 3]>>>,
        max_samples: usize,
    ) -> Result<Self, ToolpathError> {
        Self::from_contact_grid_with_pattern(rows, max_samples, RasterPattern::Unidirectional)
    }

    /// G25.2: invierte exclusivamente el orden de VISITA de pasadas impares
    /// en zigzag, sin recalcular ni modificar ninguno de los contactos f64.
    /// Los `None` continuan separando los segmentos: nunca se crean enlaces.
    pub fn from_contact_grid_with_pattern(
        rows: Vec<Vec<Option<[f64; 3]>>>,
        max_samples: usize,
        pattern: RasterPattern,
    ) -> Result<Self, ToolpathError> {
        if rows.is_empty() || rows[0].is_empty() {
            return Err(ToolpathError::EmptyRaster);
        }
        let raster_passes = rows.len();
        let samples_per_pass = rows[0].len();
        let total = raster_passes
            .checked_mul(samples_per_pass)
            .ok_or(ToolpathError::ArithmeticRange)?;
        if max_samples == 0 || total > max_samples {
            return Err(ToolpathError::SampleLimitExceeded);
        }
        let mut segments = Vec::new();
        let mut contact_points = 0usize;
        for (pass_index, mut row) in rows.into_iter().enumerate() {
            if row.len() != samples_per_pass {
                return Err(ToolpathError::NonRectangularRaster);
            }
            let reverse = pattern == RasterPattern::Zigzag && pass_index % 2 == 1;
            if reverse {
                row.reverse();
            }
            let mut active: Option<MotionSegment> = None;
            for (visit_index, hit) in row.into_iter().enumerate() {
                // Este indice referencia siempre la muestra XY del raster original.
                let sample_index = if reverse {
                    samples_per_pass - 1 - visit_index
                } else {
                    visit_index
                };
                match hit {
                    Some(position) => {
                        if !position.iter().all(|coordinate| coordinate.is_finite()) {
                            return Err(ToolpathError::NonFinitePoint);
                        }
                        contact_points += 1; // <= total <= max_samples
                        if let Some(segment) = &mut active {
                            segment.points.push(position);
                        } else {
                            active = Some(MotionSegment {
                                kind: MotionKind::Cutting,
                                pass_index,
                                first_sample: sample_index,
                                points: vec![position],
                            });
                        }
                    }
                    None => {
                        if let Some(segment) = active.take() {
                            segments.push(segment);
                        }
                    }
                }
            }
            if let Some(segment) = active.take() {
                segments.push(segment);
            }
        }
        Ok(Self {
            segments,
            pattern,
            raster_passes,
            samples_per_pass,
            contact_points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(x: f64, z: f64) -> Option<[f64; 3]> {
        Some([x, 0.0, z])
    }

    #[test]
    fn contiguous_samples_form_one_cutting_segment_per_pass() {
        let path = Toolpath::from_contact_grid(
            vec![
                vec![hit(0.0, 5.0), hit(1.0, 5.0)],
                vec![hit(0.0, 6.0), hit(1.0, 6.0)],
            ],
            4,
        )
        .unwrap();
        assert_eq!(
            (
                path.raster_passes,
                path.samples_per_pass,
                path.contact_points
            ),
            (2, 2, 4)
        );
        assert_eq!(path.segments.len(), 2);
        assert_eq!(path.segments[0].kind, MotionKind::Cutting);
        assert_eq!(
            (path.segments[0].pass_index, path.segments[0].first_sample),
            (0, 0)
        );
        assert_eq!(
            (path.segments[1].pass_index, path.segments[1].first_sample),
            (1, 0)
        );
    }

    #[test]
    fn gaps_never_create_bridge_moves() {
        let path = Toolpath::from_contact_grid(
            vec![vec![
                hit(0.0, 5.0),
                None,
                hit(2.0, 6.0),
                hit(3.0, 7.0),
                None,
                hit(5.0, 8.0),
            ]],
            6,
        )
        .unwrap();
        assert_eq!(path.contact_points, 4);
        assert_eq!(path.segments.len(), 3);
        assert_eq!(
            path.segments
                .iter()
                .map(|p| (p.first_sample, p.points.len()))
                .collect::<Vec<_>>(),
            vec![(0, 1), (2, 2), (5, 1)]
        );
    }

    #[test]
    fn completely_disconnected_passes_have_no_invented_links() {
        let path = Toolpath::from_contact_grid(
            vec![
                vec![hit(0.0, 1.0), hit(1.0, 2.0)],
                vec![None, None],
                vec![hit(0.0, 3.0), None],
            ],
            6,
        )
        .unwrap();
        assert_eq!(path.segments.len(), 2);
        assert_eq!(path.segments[0].pass_index, 0);
        assert_eq!(path.segments[1].pass_index, 2);
    }

    #[test]
    fn no_contact_is_valid_but_has_no_motion() {
        let path =
            Toolpath::from_contact_grid(vec![vec![None, None], vec![None, None]], 4).unwrap();
        assert_eq!(path.contact_points, 0);
        assert!(path.segments.is_empty());
        assert_eq!(path.raster_passes, 2);
    }

    #[test]
    fn rejects_empty_ragged_and_nonfinite_rasters() {
        assert_eq!(
            Toolpath::from_contact_grid(vec![], 10).unwrap_err(),
            ToolpathError::EmptyRaster
        );
        assert_eq!(
            Toolpath::from_contact_grid(vec![vec![]], 10).unwrap_err(),
            ToolpathError::EmptyRaster
        );
        assert_eq!(
            Toolpath::from_contact_grid(vec![vec![None], vec![None, None]], 10).unwrap_err(),
            ToolpathError::NonRectangularRaster
        );
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Toolpath::from_contact_grid(vec![vec![Some([0.0, bad, 1.0])]], 10).unwrap_err(),
                ToolpathError::NonFinitePoint
            );
        }
    }

    #[test]
    fn respects_exact_sample_budget() {
        let grid = vec![vec![None, None], vec![None, None]];
        assert_eq!(
            Toolpath::from_contact_grid(grid.clone(), 3).unwrap_err(),
            ToolpathError::SampleLimitExceeded
        );
        assert_eq!(
            Toolpath::from_contact_grid(grid.clone(), 0).unwrap_err(),
            ToolpathError::SampleLimitExceeded
        );
        assert!(Toolpath::from_contact_grid(grid, 4).is_ok());
    }

    #[test]
    fn preserves_coordinate_bits_and_scan_order() {
        let small = f64::from_bits(1);
        let negative_zero = -0.0;
        let path = Toolpath::from_contact_grid(
            vec![vec![
                Some([negative_zero, small, 2.0]),
                Some([1.0, 3.0, -0.0]),
            ]],
            2,
        )
        .unwrap();
        assert_eq!(
            path.segments[0].points[0][0].to_bits(),
            negative_zero.to_bits()
        );
        assert_eq!(path.segments[0].points[0][1].to_bits(), small.to_bits());
        assert_eq!(path.segments[0].points[1][2].to_bits(), (-0.0f64).to_bits());
    }
}

#[cfg(test)]
mod g25_2_pattern_tests {
    use super::*;

    fn hit(x: f64, y: f64) -> Option<[f64; 3]> {
        Some([x, y, -0.0])
    }

    #[test]
    fn original_constructor_is_bit_identical_to_explicit_unidirectional() {
        let rows = vec![
            vec![hit(1.0, 1.0), None, hit(3.0, 1.0)],
            vec![hit(1.0, 2.0), hit(2.0, 2.0), hit(3.0, 2.0)],
        ];
        let baseline = Toolpath::from_contact_grid(rows.clone(), 6).unwrap();
        let explicit =
            Toolpath::from_contact_grid_with_pattern(rows, 6, RasterPattern::Unidirectional)
                .unwrap();
        assert_eq!(baseline, explicit);
        assert_eq!(baseline.pattern, RasterPattern::Unidirectional);
        assert_eq!(
            baseline.segments[1].points[0][2].to_bits(),
            (-0.0_f64).to_bits()
        );
    }

    #[test]
    fn zigzag_reverses_only_odd_pass_and_keeps_original_sample_indices() {
        let rows = vec![
            vec![hit(0.0, 0.0), hit(1.0, 0.0), hit(2.0, 0.0)],
            vec![hit(0.0, 1.0), hit(1.0, 1.0), hit(2.0, 1.0)],
            vec![hit(0.0, 2.0), hit(1.0, 2.0), hit(2.0, 2.0)],
        ];
        let zigzag =
            Toolpath::from_contact_grid_with_pattern(rows.clone(), 9, RasterPattern::Zigzag)
                .unwrap();
        let forward = Toolpath::from_contact_grid(rows, 9).unwrap();
        assert_eq!(zigzag.contact_points, forward.contact_points);
        assert_eq!((zigzag.raster_passes, zigzag.samples_per_pass), (3, 3));
        assert_eq!(zigzag.segments.len(), 3);
        assert_eq!(zigzag.segments[0].points, forward.segments[0].points);
        assert_eq!(zigzag.segments[2].points, forward.segments[2].points);
        assert_eq!(zigzag.segments[1].first_sample, 2);
        assert_eq!(zigzag.segments[1].points[0][0], 2.0);
        assert_eq!(zigzag.segments[1].points[2][0], 0.0);
    }

    #[test]
    fn zigzag_gaps_reverse_segment_sequence_without_bridges() {
        let raster = vec![
            vec![hit(0.0, 0.0), None, hit(2.0, 0.0), hit(3.0, 0.0)],
            vec![hit(0.0, 1.0), hit(1.0, 1.0), None, hit(3.0, 1.0)],
        ];
        let path =
            Toolpath::from_contact_grid_with_pattern(raster, 8, RasterPattern::Zigzag).unwrap();
        assert_eq!(path.contact_points, 6);
        assert_eq!(path.segments.len(), 4);
        assert_eq!(
            path.segments
                .iter()
                .map(|s| (s.pass_index, s.first_sample, s.points.len()))
                .collect::<Vec<_>>(),
            vec![(0, 0, 1), (0, 2, 2), (1, 3, 1), (1, 1, 2)]
        );
        assert_eq!(path.segments[3].points[0][0], 1.0);
        assert_eq!(path.segments[3].points[1][0], 0.0);
        assert!(path.segments.iter().all(|s| s.kind == MotionKind::Cutting));
    }

    #[test]
    fn patterns_reject_same_errors_without_partial_motion() {
        for pattern in [RasterPattern::Unidirectional, RasterPattern::Zigzag] {
            assert_eq!(
                Toolpath::from_contact_grid_with_pattern(vec![vec![hit(0.0, 0.0)]], 0, pattern)
                    .unwrap_err(),
                ToolpathError::SampleLimitExceeded
            );
            assert_eq!(
                Toolpath::from_contact_grid_with_pattern(
                    vec![vec![hit(0.0, 0.0)], vec![hit(0.0, 1.0), None]],
                    3,
                    pattern,
                )
                .unwrap_err(),
                ToolpathError::NonRectangularRaster
            );
            assert_eq!(
                Toolpath::from_contact_grid_with_pattern(
                    vec![vec![hit(0.0, 0.0)], vec![Some([1.0, f64::NAN, 0.0])]],
                    2,
                    pattern,
                )
                .unwrap_err(),
                ToolpathError::NonFinitePoint
            );
        }
    }
}

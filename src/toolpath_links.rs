//! G25.3: propuesta de recorridos entre tramos de contacto, SIN G-code.
//!
//! Los movimientos azules son TRAVESIAS horizontales tipo G0 a una Z de
//! despeje fija; aproximaciones, retiradas y corte son movimientos controlados
//! (rojos). Esta geometria NO certifica seguridad: carece de comprobacion de
//! fijaciones, mango, portaherramientas, material restante y maquina.

use crate::toolpath::Toolpath;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannedMoveKind {
    /// Desplazamiento XY propuesto a altura de despeje constante.
    RapidTraverse,
    /// Descenso controlado hasta el primer contacto del tramo.
    Approach,
    /// Contactos consecutivos conservados bit a bit desde el Contact Solver.
    Cutting,
    /// Subida controlada antes de permitir una travesia.
    Retract,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedMove {
    pub kind: PlannedMoveKind,
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub pass_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlannedToolpath {
    pub moves: Vec<PlannedMove>,
    pub reference_z: f64,
    pub approach_z: f64,
    pub clearance_z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanningError {
    InvalidHeights,
    NonFiniteContact,
    ArithmeticRange,
}

impl fmt::Display for PlanningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeights => {
                f.write_str("Alturas CAM invalidas: despeje > aproximacion > 0")
            }
            Self::NonFiniteContact => f.write_str("Contacto CAM no finito o vacio"),
            Self::ArithmeticRange => f.write_str("Alturas CAM fuera de rango aritmetico"),
        }
    }
}
impl std::error::Error for PlanningError {}

impl PlannedToolpath {
    /// Dos cotas relativas a Zref = max(cima STL, cima bruto, puntas calculadas).
    /// Las operaciones anteriores y el orden zigzag quedan intactos.
    /// La primera posicion se supone en el inicio del primer segmento a Zsegura:
    /// NO se genera un movimiento desde origen maquina o posicion actual.
    pub fn from_toolpath(
        path: &Toolpath,
        model_stock_top_z: f64,
        approach_above_top_mm: f64,
        clearance_above_top_mm: f64,
    ) -> Result<Self, PlanningError> {
        if !model_stock_top_z.is_finite()
            || !approach_above_top_mm.is_finite()
            || !clearance_above_top_mm.is_finite()
            || approach_above_top_mm <= 0.0
            || clearance_above_top_mm <= approach_above_top_mm
            || clearance_above_top_mm > 1_000_000.0
        {
            return Err(PlanningError::InvalidHeights);
        }

        let mut reference_z = model_stock_top_z;
        for segment in &path.segments {
            if segment.points.is_empty() {
                return Err(PlanningError::NonFiniteContact);
            }
            for position in &segment.points {
                if position.iter().any(|v| !v.is_finite()) {
                    return Err(PlanningError::NonFiniteContact);
                }
                reference_z = reference_z.max(position[2]);
            }
        }
        let approach_z = reference_z + approach_above_top_mm;
        let clearance_z = reference_z + clearance_above_top_mm;
        if !approach_z.is_finite()
            || !clearance_z.is_finite()
            || !(reference_z < approach_z && approach_z < clearance_z)
        {
            return Err(PlanningError::ArithmeticRange);
        }

        let mut moves = Vec::new();
        let mut previous_safe: Option<[f64; 3]> = None;
        for segment in &path.segments {
            let first = segment.points[0];
            let last = *segment.points.last().expect("Segmento verificado no vacio");
            let safe_start = [first[0], first[1], clearance_z];
            let approach_start = [first[0], first[1], approach_z];
            let approach_end = [last[0], last[1], approach_z];
            let safe_end = [last[0], last[1], clearance_z];
            // Nunca hay G0 por debajo de clearance_z, ni un corte sobre un hueco.
            if let Some(safe) = previous_safe
                && safe != safe_start
            {
                moves.push(PlannedMove {
                    kind: PlannedMoveKind::RapidTraverse,
                    from: safe,
                    to: safe_start,
                    pass_index: segment.pass_index,
                });
            }
            for (from, to) in [(safe_start, approach_start), (approach_start, first)] {
                moves.push(PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from,
                    to,
                    pass_index: segment.pass_index,
                });
            }
            for pair in segment.points.windows(2) {
                moves.push(PlannedMove {
                    kind: PlannedMoveKind::Cutting,
                    from: pair[0],
                    to: pair[1],
                    pass_index: segment.pass_index,
                });
            }
            for (from, to) in [(last, approach_end), (approach_end, safe_end)] {
                moves.push(PlannedMove {
                    kind: PlannedMoveKind::Retract,
                    from,
                    to,
                    pass_index: segment.pass_index,
                });
            }
            previous_safe = Some(safe_end);
        }
        Ok(Self {
            moves,
            reference_z,
            approach_z,
            clearance_z,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::{RasterPattern, Toolpath};

    fn toolpath(rows: Vec<Vec<Option<[f64; 3]>>>, pattern: RasterPattern) -> Toolpath {
        let total = rows.len() * rows[0].len();
        Toolpath::from_contact_grid_with_pattern(rows, total, pattern).unwrap()
    }
    fn p(x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
        Some([x, y, z])
    }

    #[test]
    fn blue_traverses_only_at_clearance_and_all_movements_are_continuous() {
        let path = toolpath(
            vec![
                vec![p(0.0, 0.0, 5.0), p(1.0, 0.0, 7.0)],
                vec![p(1.0, 1.0, 6.0), p(0.0, 1.0, 5.0)],
            ],
            RasterPattern::Zigzag,
        );
        let plan = PlannedToolpath::from_toolpath(&path, 8.0, 2.0, 10.0).unwrap();
        assert_eq!(
            (plan.reference_z, plan.approach_z, plan.clearance_z),
            (8.0, 10.0, 18.0)
        );
        let rapid: Vec<_> = plan
            .moves
            .iter()
            .filter(|m| m.kind == PlannedMoveKind::RapidTraverse)
            .collect();
        assert_eq!(rapid.len(), 1);
        assert_eq!(rapid[0].from[2].to_bits(), plan.clearance_z.to_bits());
        assert_eq!(rapid[0].to[2].to_bits(), plan.clearance_z.to_bits());
        for pair in plan.moves.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
    }

    #[test]
    fn no_bridge_across_contact_gap_and_one_point_segment_is_safe_preview() {
        let path = toolpath(
            vec![vec![p(0.0, 0.0, 1.0), None, p(2.0, 0.0, 2.0)]],
            RasterPattern::Unidirectional,
        );
        let plan = PlannedToolpath::from_toolpath(&path, 3.0, 1.0, 5.0).unwrap();
        assert_eq!(
            plan.moves
                .iter()
                .filter(|m| m.kind == PlannedMoveKind::Cutting)
                .count(),
            0
        );
        assert_eq!(
            plan.moves
                .iter()
                .filter(|m| m.kind == PlannedMoveKind::RapidTraverse)
                .count(),
            1
        );
        assert_eq!(plan.moves.len(), 9);
    }

    #[test]
    fn zigzag_reuses_original_order_and_keeps_contact_bits() {
        let path = toolpath(
            vec![
                vec![p(0.0, 0.0, 1.0), p(1.0, 0.0, 1.0)],
                vec![p(0.0, 1.0, -0.0), p(1.0, 1.0, -0.0)],
            ],
            RasterPattern::Zigzag,
        );
        let plan = PlannedToolpath::from_toolpath(&path, 1.0, 1.0, 4.0).unwrap();
        let cut: Vec<_> = plan
            .moves
            .iter()
            .filter(|m| m.kind == PlannedMoveKind::Cutting)
            .collect();
        assert_eq!(cut.len(), 2);
        assert_eq!(cut[0].from, [0.0, 0.0, 1.0]);
        assert_eq!(cut[1].from[0], 1.0);
        assert_eq!(cut[1].to[0], 0.0);
        assert_eq!(cut[1].from[2].to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn stock_top_and_contact_tip_both_influence_reference() {
        let path = toolpath(vec![vec![p(0.0, 0.0, 12.0)]], RasterPattern::Unidirectional);
        let lower_stock = PlannedToolpath::from_toolpath(&path, 5.0, 1.0, 5.0).unwrap();
        assert_eq!(lower_stock.reference_z, 12.0);
        let higher_stock = PlannedToolpath::from_toolpath(&path, 20.0, 1.0, 5.0).unwrap();
        assert_eq!(higher_stock.reference_z, 20.0);
    }

    #[test]
    fn invalid_heights_or_overflow_are_rejected_without_moves() {
        let path = toolpath(vec![vec![p(0.0, 0.0, 2.0)]], RasterPattern::Unidirectional);
        for (approach, clearance) in [
            (0.0, 5.0),
            (2.0, 2.0),
            (3.0, 2.0),
            (f64::NAN, 10.0),
            (f64::INFINITY, 10.0),
            (2.0, f64::NAN),
            (2.0, 1_000_001.0),
        ] {
            assert_eq!(
                PlannedToolpath::from_toolpath(&path, 0.0, approach, clearance).unwrap_err(),
                PlanningError::InvalidHeights
            );
        }
        assert_eq!(
            PlannedToolpath::from_toolpath(&path, f64::MAX, 1.0, 10.0).unwrap_err(),
            PlanningError::ArithmeticRange
        );
    }

    #[test]
    fn no_contact_means_no_moves() {
        let path = toolpath(vec![vec![None, None]], RasterPattern::Unidirectional);
        let plan = PlannedToolpath::from_toolpath(&path, 1.0, 2.0, 10.0).unwrap();
        assert_eq!(plan.moves.len(), 0);
    }

    #[test]
    fn invalid_external_segment_is_rejected() {
        let mut path = toolpath(vec![vec![p(0.0, 0.0, 1.0)]], RasterPattern::Unidirectional);
        path.segments[0].points[0][0] = f64::NAN;
        assert_eq!(
            PlannedToolpath::from_toolpath(&path, 1.0, 1.0, 4.0).unwrap_err(),
            PlanningError::NonFiniteContact
        );
    }
}

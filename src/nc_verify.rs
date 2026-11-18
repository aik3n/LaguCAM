//! Advertencias geometricas conservadoras sobre el NC final interpretado.
//!
//! NO es un verificador de colisiones ni una certificacion de seguridad CNC.
//! Compara las envolventes de herramienta y del BRUTO ORIGINAL; no conoce el
//! material eliminado en pasadas anteriores, fijaciones ni portaherramientas.
//! Las advertencias G0 son posibles intersecciones, no colisiones confirmadas.
//! Las coordenadas NC son absolutas en mm de trabajo y se convierten a modelo
//! mediante la misma transformacion validada del reproductor.

use crate::nc_contract::{NcEventKind, NcMotionKind, NcSpindleMode, NcTrace};

const MAX_REPORTED_ISSUES: usize = 128;
const EPS_MM: f64 = 1.0e-8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockEnvelope {
    pub min_mm: [f64; 3],
    pub max_mm: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutterEnvelope {
    pub radius_mm: f64,
    pub cutting_length_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationSeverity {
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationCode {
    RapidStockCandidate,
    FeedBelowStockBase,
    FeedOutsideStockXY,
    FeedWithStoppedSpindle,
    UnverifiedToolSelection,
}

impl VerificationCode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RapidStockCandidate => "G0 posiblemente en el material",
            Self::FeedBelowStockBase => "G1 por debajo de la base del bruto",
            Self::FeedOutsideStockXY => "G1 fuera del contorno XY del bruto",
            Self::FeedWithStoppedSpindle => "G1 cerca del bruto con husillo parado",
            Self::UnverifiedToolSelection => "Orden T sin correspondencia verificada",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationIssue {
    pub severity: VerificationSeverity,
    pub code: VerificationCode,
    pub source_line: usize,
    /// Indice cero-based del movimiento NC al que corresponde el aviso.
    /// None para un evento modal sin movimiento.
    pub move_index: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerificationReport {
    pub checked_moves: usize,
    pub rapid_stock_candidates: usize,
    pub feed_below_base: usize,
    pub feed_outside_xy: usize,
    pub feed_spindle_stopped: usize,
    pub unverified_tool_commands: usize,
    pub issues: Vec<VerificationIssue>,
    pub omitted_issues: usize,
}

impl VerificationReport {
    fn push(
        &mut self,
        severity: VerificationSeverity,
        code: VerificationCode,
        source_line: usize,
        move_index: Option<usize>,
    ) {
        if self.issues.len() == MAX_REPORTED_ISSUES {
            self.omitted_issues += 1;
        } else {
            self.issues.push(VerificationIssue {
                severity,
                code,
                source_line,
                move_index,
            });
        }
    }
    pub fn total_issues(&self) -> usize {
        self.issues.len() + self.omitted_issues
    }
}

fn validate_envelopes(stock: StockEnvelope, cutter: CutterEnvelope) -> Result<(), String> {
    if !(0..3).all(|axis| {
        stock.min_mm[axis].is_finite()
            && stock.max_mm[axis].is_finite()
            && stock.min_mm[axis] < stock.max_mm[axis]
    }) {
        return Err("Limites del bruto no validos para verificar NC".into());
    }
    if !cutter.radius_mm.is_finite()
        || cutter.radius_mm <= 0.0
        || !cutter.cutting_length_mm.is_finite()
        || cutter.cutting_length_mm <= 0.0
    {
        return Err("Radio o longitud de corte no validos para verificar NC".into());
    }
    let limits = [
        stock.min_mm[0] - cutter.radius_mm,
        stock.min_mm[1] - cutter.radius_mm,
        stock.min_mm[2] - cutter.cutting_length_mm,
        stock.max_mm[0] + cutter.radius_mm,
        stock.max_mm[1] + cutter.radius_mm,
        stock.max_mm[2],
    ];
    if !limits.iter().all(|v| v.is_finite()) {
        return Err("Envolvente fuera del rango numerico para verificar NC".into());
    }
    Ok(())
}

/// Interseccion de segmento con caja AABB; no subdivide el movimiento ni
/// infiere la forma real del material ya retirado.
fn segment_box(
    from: [f64; 3],
    to: [f64; 3],
    low: [f64; 3],
    high: [f64; 3],
    dimensions: usize,
) -> bool {
    let (mut enter, mut exit) = (0.0_f64, 1.0_f64);
    for axis in 0..dimensions {
        let delta = to[axis] - from[axis];
        if delta == 0.0 {
            if from[axis] < low[axis] || from[axis] > high[axis] {
                return false;
            }
        } else {
            let a = (low[axis] - from[axis]) / delta;
            let b = (high[axis] - from[axis]) / delta;
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

pub fn verify_trace(
    trace: &NcTrace,
    stock: StockEnvelope,
    cutter: CutterEnvelope,
    mut work_to_model: impl FnMut([f64; 3]) -> Result<[f64; 3], String>,
) -> Result<VerificationReport, String> {
    trace
        .validate()
        .map_err(|e| format!("Traza NC invalida: {e}"))?;
    validate_envelopes(stock, cutter)?;
    let min = [
        stock.min_mm[0] - cutter.radius_mm,
        stock.min_mm[1] - cutter.radius_mm,
        stock.min_mm[2] - cutter.cutting_length_mm + EPS_MM,
    ];
    let max = [
        stock.max_mm[0] + cutter.radius_mm,
        stock.max_mm[1] + cutter.radius_mm,
        stock.max_mm[2] - EPS_MM,
    ];
    let mut report = VerificationReport::default();
    let mut spindle = trace.initial.spindle_mode;
    for event in &trace.events {
        match event.kind {
            NcEventKind::Spindle(mode) => spindle = mode,
            NcEventKind::SelectTool(_) => {
                report.unverified_tool_commands += 1;
                report.push(
                    VerificationSeverity::Warning,
                    VerificationCode::UnverifiedToolSelection,
                    event.source_line,
                    None,
                );
            }
            NcEventKind::ToolChange(_) => {
                return Err(format!(
                    "Linea {}: cambio de herramienta sin correspondencia verificada",
                    event.source_line
                ));
            }
            NcEventKind::Motion(motion) => {
                let index = report.checked_moves;
                report.checked_moves += 1;
                let from = work_to_model(motion.from_work_mm)
                    .map_err(|e| format!("Linea {}: {e}", event.source_line))?;
                let to = work_to_model(motion.to_work_mm)
                    .map_err(|e| format!("Linea {}: {e}", event.source_line))?;
                if !from.iter().chain(to.iter()).all(|x| x.is_finite())
                    || !(0..3).all(|axis| (to[axis] - from[axis]).is_finite())
                {
                    return Err(format!(
                        "Linea {}: conversion a modelo no finita",
                        event.source_line
                    ));
                }
                let nearby_xy = segment_box(from, to, min, max, 2);
                let maybe_stock = nearby_xy && segment_box(from, to, min, max, 3);
                match motion.kind {
                    NcMotionKind::Rapid => {
                        if maybe_stock {
                            report.rapid_stock_candidates += 1;
                            report.push(
                                VerificationSeverity::Warning,
                                VerificationCode::RapidStockCandidate,
                                event.source_line,
                                Some(index),
                            );
                        }
                    }
                    NcMotionKind::LinearFeed => {
                        if nearby_xy && from[2].min(to[2]) < stock.min_mm[2] - EPS_MM {
                            report.feed_below_base += 1;
                            report.push(
                                VerificationSeverity::Warning,
                                VerificationCode::FeedBelowStockBase,
                                event.source_line,
                                Some(index),
                            );
                        }
                        if !nearby_xy {
                            report.feed_outside_xy += 1;
                            report.push(
                                VerificationSeverity::Info,
                                VerificationCode::FeedOutsideStockXY,
                                event.source_line,
                                Some(index),
                            );
                        }
                        if maybe_stock && spindle == NcSpindleMode::Stopped {
                            report.feed_spindle_stopped += 1;
                            report.push(
                                VerificationSeverity::Warning,
                                VerificationCode::FeedWithStoppedSpindle,
                                event.source_line,
                                Some(index),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nc_contract::{NcDistanceMode, NcInitialState, NcUnits};
    use crate::nc_interpreter::interpret_program;

    fn initial(xyz: [f64; 3]) -> NcInitialState {
        NcInitialState {
            position_work_mm: xyz,
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        }
    }
    fn stock() -> StockEnvelope {
        StockEnvelope {
            min_mm: [0.0; 3],
            max_mm: [10.0; 3],
        }
    }
    fn cutter() -> CutterEnvelope {
        CutterEnvelope {
            radius_mm: 1.0,
            cutting_length_mm: 4.0,
        }
    }
    fn check(code: &str, xyz: [f64; 3]) -> VerificationReport {
        let trace = interpret_program(code, initial(xyz)).unwrap();
        verify_trace(&trace, stock(), cutter(), Ok).unwrap()
    }

    #[test]
    fn safe_rapid_above_stock_has_no_candidate() {
        let result = check("G21 G90 G94\nG0 X5 Y5 Z20\nM30\n", [-10.0, 5.0, 20.0]);
        assert_eq!(result.checked_moves, 1);
        assert_eq!(result.rapid_stock_candidates, 0);
        assert_eq!(result.total_issues(), 0);
    }
    #[test]
    fn rapid_crossing_original_stock_is_reported_with_exact_line() {
        let result = check(
            "G21 G90 G94\nG0 X5 Y5 Z15\nG0 Z4\nM30\n",
            [-10.0, 5.0, 15.0],
        );
        assert_eq!(result.rapid_stock_candidates, 1);
        assert_eq!(result.issues[0].source_line, 3);
        assert_eq!(result.issues[0].move_index, Some(1));
    }
    #[test]
    fn feed_below_stock_base_is_reported() {
        let result = check(
            "G21 G90 G94\nS8000 M3\nG1 X5 Y5 Z-1 F100\nM5 M30\n",
            [5.0, 5.0, 5.0],
        );
        assert_eq!(result.feed_below_base, 1);
        assert_eq!(result.issues[0].code, VerificationCode::FeedBelowStockBase);
    }
    #[test]
    fn lateral_feed_outside_stock_is_information_not_a_collision() {
        let result = check(
            "G21 G90 G94\nS8000 M3\nG1 X20 Y20 Z5 F100\nM30\n",
            [18.0, 20.0, 5.0],
        );
        assert_eq!(result.feed_outside_xy, 1);
        assert_eq!(result.issues[0].severity, VerificationSeverity::Info);
    }
    #[test]
    fn feed_with_stopped_spindle_is_not_ignored() {
        let result = check("G21 G90 G94\nG1 X5 Y5 Z5 F100\nM30\n", [5.0, 5.0, 15.0]);
        assert_eq!(result.feed_spindle_stopped, 1);
    }
    #[test]
    fn modal_spindle_state_is_preserved_through_start_and_end() {
        let result = check(
            "G21 G90 G94\nS8000 M3\nG1 X5 Y5 Z5 F100\nM5\nG1 Z4\nM30\n",
            [5.0, 5.0, 15.0],
        );
        assert_eq!(result.feed_spindle_stopped, 1);
        assert_eq!(result.issues.last().unwrap().source_line, 5);
    }
    #[test]
    fn inch_nc_uses_physical_mm_positions_for_verification() {
        let result = check("G20 G90 G94\nG0 X0.2 Y0.2 Z0.2\nM30\n", [5.08, 5.08, 20.0]);
        assert_eq!(result.rapid_stock_candidates, 1);
        assert_eq!(result.issues[0].source_line, 2);
    }
    #[test]
    fn converted_coordinates_and_invalid_inputs_fail_closed() {
        let trace = interpret_program("G21 G90 G94\nG0 X5 Y5 Z5\nM30\n", initial([0.0, 0.0, 20.0]))
            .unwrap();
        assert!(
            verify_trace(
                &trace,
                stock(),
                CutterEnvelope {
                    radius_mm: 0.0,
                    cutting_length_mm: 1.0
                },
                Ok
            )
            .is_err()
        );
        assert!(
            verify_trace(&trace, stock(), cutter(), |_| Err(
                "cero de trabajo invalido".into()
            ))
            .is_err()
        );
        let shifted = verify_trace(&trace, stock(), cutter(), |p| {
            Ok([p[0] + 100.0, p[1] + 100.0, p[2]])
        })
        .unwrap();
        assert_eq!(shifted.rapid_stock_candidates, 0);
    }
    #[test]
    fn tool_commands_are_not_silently_assumed_equivalent() {
        let trace = interpret_program("G21 G90 G94\nT3\nM30\n", initial([0.0; 3])).unwrap();
        let report = verify_trace(&trace, stock(), cutter(), Ok).unwrap();
        assert_eq!(report.unverified_tool_commands, 1);
    }
}

//! G25.4a: verificacion ESTRUCTURAL de movimientos, sin interfaz ni OpenGL.
//!
//! Un resultado favorable NO prueba ausencia de colisiones ni autoriza G-code:
//! no se conocen fijaciones, portaherramientas, maquina o material restante.
//! La entrada es inmutable. Los errores identifican un movimiento concreto.

use crate::toolpath_links::{PlannedMove, PlannedMoveKind, PlannedToolpath};

pub const MAX_VALIDATION_MOVES: usize = 1_000_000;
const MAX_ISSUES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueCode {
    InvalidHeights,
    TooManyMoves,
    EmptyOperation,
    NoCutting,
    InvalidCoordinates,
    NumericOverflow,
    Discontinuity,
    ZeroLength,
    WrongClearance,
    NonVerticalControlledMove,
    WrongVerticalDirection,
    CuttingAboveReference,
    WrongOrder,
    WrongPass,
    BackwardPass,
    UnfinishedSequence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: IssueCode,
    /// Indice de movimiento, comenzando en cero; None para problemas globales.
    pub move_index: Option<usize>,
    pub pass_index: Option<usize>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub checked_moves: usize,
    pub cutting_moves: usize,
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// NO significa "apto para maquina": solo coherencia de la secuencia.
    pub fn is_structurally_valid(&self) -> bool {
        !self.issues.iter().any(|x| x.severity == Severity::Error)
    }

    fn push(
        &mut self,
        severity: Severity,
        code: IssueCode,
        at: Option<(usize, usize)>,
        detail: impl Into<String>,
    ) {
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(ValidationIssue {
                severity,
                code,
                move_index: at.map(|x| x.0),
                pass_index: at.map(|x| x.1),
                detail: detail.into(),
            });
        }
    }

    fn error(&mut self, code: IssueCode, at: Option<(usize, usize)>, detail: impl Into<String>) {
        self.push(Severity::Error, code, at, detail);
    }

    fn warning(&mut self, code: IssueCode, detail: &'static str) {
        self.push(Severity::Warning, code, None, detail);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    AtClearance,
    ToContact,
    Cutting,
    ToClearance,
}

fn vertical(m: &PlannedMove) -> bool {
    m.from[0] == m.to[0] && m.from[1] == m.to[1]
}

/// Valida el contrato actual de G25.3 (dos aproximaciones, cortes, dos
/// retiradas; enlaces solo horizontales a Zdespeje). No altera la trayectoria.
/// No inspecciona STL ni material restante; no certifica seguridad de maquina.
pub fn validate_planned_toolpath(path: &PlannedToolpath) -> ValidationReport {
    let mut report = ValidationReport::default();
    if !path.reference_z.is_finite()
        || !path.approach_z.is_finite()
        || !path.clearance_z.is_finite()
        || !(path.reference_z < path.approach_z && path.approach_z < path.clearance_z)
        || !(path.approach_z - path.reference_z).is_finite()
        || !(path.clearance_z - path.approach_z).is_finite()
    {
        report.error(
            IssueCode::InvalidHeights,
            None,
            format!(
                "Z de referencia {}, aproximacion {} y despeje {} deben ser finitas y crecientes",
                path.reference_z, path.approach_z, path.clearance_z
            ),
        );
        return report;
    }
    if path.moves.len() > MAX_VALIDATION_MOVES {
        report.error(
            IssueCode::TooManyMoves,
            None,
            format!(
                "{} movimientos: limite {MAX_VALIDATION_MOVES}",
                path.moves.len()
            ),
        );
        return report;
    }
    if path.moves.is_empty() {
        report.warning(
            IssueCode::EmptyOperation,
            "No existen movimientos ni cortes",
        );
        return report;
    }

    let mut phase = Phase::AtClearance;
    let mut current_pass: Option<usize> = None;
    let mut completed_pass: Option<usize> = None;
    let mut done_segment = false;
    let mut pending_rapid: Option<usize> = None;
    let mut last_end: Option<[f64; 3]> = None;

    for (i, m) in path.moves.iter().enumerate() {
        let at = Some((i, m.pass_index));
        report.checked_moves += 1;
        if m.kind == PlannedMoveKind::Cutting {
            report.cutting_moves += 1;
        }
        if !m.from.iter().chain(m.to.iter()).all(|x| x.is_finite()) {
            report.error(
                IssueCode::InvalidCoordinates,
                at,
                "Movimiento con NaN o infinito",
            );
            last_end = None;
            continue;
        }
        if m.from
            .iter()
            .zip(m.to.iter())
            .any(|(a, b)| !(b - a).is_finite())
        {
            report.error(
                IssueCode::NumericOverflow,
                at,
                "Diferencia XYZ fuera del rango f64",
            );
            last_end = None;
            continue;
        }
        if let Some(last) = last_end {
            if last != m.from {
                report.error(
                    IssueCode::Discontinuity,
                    at,
                    format!("Salto: fin anterior {:?}, inicio actual {:?}", last, m.from),
                );
            }
        }
        last_end = Some(m.to);
        if m.from == m.to {
            report.error(IssueCode::ZeroLength, at, "Movimiento de longitud cero");
        }
        if phase != Phase::AtClearance
            && m.kind != PlannedMoveKind::RapidTraverse
            && current_pass.is_some_and(|pass| pass != m.pass_index)
        {
            report.error(
                IssueCode::WrongPass,
                at,
                "El indice no coincide con la pasada activa",
            );
        }
        match m.kind {
            PlannedMoveKind::RapidTraverse => {
                if m.from[2] != path.clearance_z || m.to[2] != path.clearance_z {
                    report.error(
                        IssueCode::WrongClearance,
                        at,
                        format!(
                            "G0 horizontal exige Z={} en ambos extremos; Z={} -> {}",
                            path.clearance_z, m.from[2], m.to[2]
                        ),
                    );
                }
                if phase != Phase::AtClearance || !done_segment || pending_rapid.is_some() {
                    report.error(
                        IssueCode::WrongOrder,
                        at,
                        "G0 solo entre tramos completamente retirados",
                    );
                } else {
                    pending_rapid = Some(m.pass_index);
                }
            }
            PlannedMoveKind::Approach | PlannedMoveKind::Retract => {
                if !vertical(m) {
                    report.error(
                        IssueCode::NonVerticalControlledMove,
                        at,
                        "Aproximacion/retirada debe tener XY constante",
                    );
                }
                let descending = m.kind == PlannedMoveKind::Approach;
                if (descending && m.to[2] >= m.from[2]) || (!descending && m.to[2] <= m.from[2]) {
                    report.error(
                        IssueCode::WrongVerticalDirection,
                        at,
                        "Aproximacion debe bajar; retirada debe subir",
                    );
                }
                match (phase, m.kind) {
                    (Phase::AtClearance, PlannedMoveKind::Approach) => {
                        if m.from[2] != path.clearance_z || m.to[2] != path.approach_z {
                            report.error(
                                IssueCode::WrongClearance,
                                at,
                                "Primera aproximacion: Zdespeje -> Zapróximacion",
                            );
                        }
                        if let Some(pass) = completed_pass {
                            if m.pass_index < pass {
                                report.error(
                                    IssueCode::BackwardPass,
                                    at,
                                    "El indice de pasada retrocede",
                                );
                            }
                        }
                        if let Some(pass) = pending_rapid {
                            if m.pass_index != pass {
                                report.error(
                                    IssueCode::WrongPass,
                                    at,
                                    "El enlace G0 apunta a otra pasada",
                                );
                            }
                        }
                        current_pass = Some(m.pass_index);
                        pending_rapid = None;
                        phase = Phase::ToContact;
                    }
                    (Phase::ToContact, PlannedMoveKind::Approach) => {
                        if m.from[2] != path.approach_z || m.to[2] > path.reference_z {
                            report.error(
                                IssueCode::WrongClearance,
                                at,
                                "Segunda aproximacion: Zapróximacion -> contacto bajo Zref",
                            );
                        }
                        phase = Phase::Cutting;
                    }
                    (Phase::Cutting, PlannedMoveKind::Retract) => {
                        if m.from[2] > path.reference_z || m.to[2] != path.approach_z {
                            report.error(
                                IssueCode::WrongClearance,
                                at,
                                "Primera retirada: contacto bajo Zref -> Zapróximacion",
                            );
                        }
                        phase = Phase::ToClearance;
                    }
                    (Phase::ToClearance, PlannedMoveKind::Retract) => {
                        if m.from[2] != path.approach_z || m.to[2] != path.clearance_z {
                            report.error(
                                IssueCode::WrongClearance,
                                at,
                                "Segunda retirada: Zapróximacion -> Zdespeje",
                            );
                        }
                        done_segment = true;
                        completed_pass = current_pass;
                        current_pass = None;
                        phase = Phase::AtClearance;
                    }
                    _ => report.error(
                        IssueCode::WrongOrder,
                        at,
                        "Aproximacion/retirada en orden incorrecto",
                    ),
                }
            }
            PlannedMoveKind::Cutting => {
                if phase != Phase::Cutting {
                    report.error(IssueCode::WrongOrder, at, "Corte sin aproximacion completa");
                }
                if m.from[2] > path.reference_z || m.to[2] > path.reference_z {
                    report.error(
                        IssueCode::CuttingAboveReference,
                        at,
                        "Corte por encima de Zref",
                    );
                }
            }
        }
    }
    if phase != Phase::AtClearance || pending_rapid.is_some() {
        report.error(
            IssueCode::UnfinishedSequence,
            None,
            "La operacion termina sin retirada completa o con un G0 pendiente",
        );
    }
    if report.cutting_moves == 0 {
        report.warning(
            IssueCode::NoCutting,
            "Hay movimientos, pero ningun segmento de corte",
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolpath::{RasterPattern, Toolpath};

    fn hit(x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
        Some([x, y, z])
    }

    fn plan(pattern: RasterPattern) -> PlannedToolpath {
        let contact = Toolpath::from_contact_grid_with_pattern(
            vec![
                vec![hit(0.0, 0.0, 1.0), hit(1.0, 0.0, 2.0)],
                vec![hit(0.0, 1.0, 1.0), hit(1.0, 1.0, 2.0)],
            ],
            4,
            pattern,
        )
        .unwrap();
        PlannedToolpath::from_toolpath(&contact, 4.0, 2.0, 10.0).unwrap()
    }
    fn has(report: &ValidationReport, code: IssueCode) -> bool {
        report.issues.iter().any(|x| x.code == code)
    }

    #[test]
    fn accepts_generated_unidirectional_and_zigzag_without_mutating() {
        for pattern in [RasterPattern::Unidirectional, RasterPattern::Zigzag] {
            let path = plan(pattern);
            let saved = path.clone();
            let report = validate_planned_toolpath(&path);
            assert!(report.is_structurally_valid(), "{:?}", report.issues);
            assert!(report.issues.is_empty());
            assert_eq!(report.checked_moves, path.moves.len());
            assert_eq!(report.cutting_moves, 2);
            assert_eq!(path, saved);
        }
    }
    #[test]
    fn catches_discontinuity_with_exact_movement_index() {
        let mut path = plan(RasterPattern::Zigzag);
        path.moves[5].to[0] += 0.25;
        let report = validate_planned_toolpath(&path);
        let issue = report
            .issues
            .iter()
            .find(|x| x.code == IssueCode::Discontinuity)
            .unwrap();
        assert_eq!((issue.move_index, issue.pass_index), (Some(6), Some(1)));
        assert!(!report.is_structurally_valid());
    }
    #[test]
    fn catches_rapid_below_clearance() {
        let mut path = plan(RasterPattern::Zigzag);
        path.moves[5].from[2] -= 1.0;
        path.moves[5].to[2] -= 1.0;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::WrongClearance
        ));
    }
    #[test]
    fn catches_wrong_sequence_and_missing_retract() {
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[0].kind = PlannedMoveKind::Cutting;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::WrongOrder
        ));
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves.pop();
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::UnfinishedSequence
        ));
    }
    #[test]
    fn catches_lateral_approach_and_wrong_vertical_direction() {
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[0].to[0] += 1.0;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::NonVerticalControlledMove
        ));
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[0].to[2] = path.moves[0].from[2] + 1.0;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::WrongVerticalDirection
        ));
    }
    #[test]
    fn catches_zero_length_and_cutting_above_reference() {
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[2].to = path.moves[2].from;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::ZeroLength
        ));
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[2].to[2] = path.reference_z + 1.0;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::CuttingAboveReference
        ));
    }
    #[test]
    fn catches_nonfinite_and_arithmetic_overflow() {
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[2].to[0] = f64::NAN;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::InvalidCoordinates
        ));
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[2].from[0] = -f64::MAX;
        path.moves[2].to[0] = f64::MAX;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::NumericOverflow
        ));
    }
    #[test]
    fn catches_invalid_heights() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut path = plan(RasterPattern::Zigzag);
            path.clearance_z = value;
            assert!(has(
                &validate_planned_toolpath(&path),
                IssueCode::InvalidHeights
            ));
        }
        let mut path = plan(RasterPattern::Zigzag);
        path.clearance_z = path.approach_z;
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::InvalidHeights
        ));
    }
    #[test]
    fn catches_wrong_and_backward_pass_indices() {
        let mut path = plan(RasterPattern::Unidirectional);
        path.moves[4].pass_index = 12;
        assert!(has(&validate_planned_toolpath(&path), IssueCode::WrongPass));
        let mut path = plan(RasterPattern::Unidirectional);
        for movement in &mut path.moves[..5] {
            movement.pass_index = 3;
        }
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::BackwardPass
        ));
    }
    #[test]
    fn empty_is_a_warning_not_an_executable_toolpath() {
        let path = Toolpath::from_contact_grid(vec![vec![None, None]], 2).unwrap();
        let planned = PlannedToolpath::from_toolpath(&path, 4.0, 2.0, 10.0).unwrap();
        let report = validate_planned_toolpath(&planned);
        assert!(report.is_structurally_valid());
        assert!(has(&report, IssueCode::EmptyOperation));
        assert_eq!(report.cutting_moves, 0);
    }
    #[test]
    fn isolated_contact_is_valid_preview_but_warns_no_cut() {
        let path = Toolpath::from_contact_grid(vec![vec![hit(0.0, 0.0, 1.0)]], 1).unwrap();
        let planned = PlannedToolpath::from_toolpath(&path, 4.0, 2.0, 10.0).unwrap();
        let report = validate_planned_toolpath(&planned);
        assert!(report.is_structurally_valid(), "{:?}", report.issues);
        assert!(has(&report, IssueCode::NoCutting));
        assert_eq!(report.checked_moves, 4);
    }
    #[test]
    fn gap_is_not_an_implicit_cutting_link() {
        let path = Toolpath::from_contact_grid(
            vec![vec![hit(0.0, 0.0, 1.0), None, hit(2.0, 0.0, 2.0)]],
            3,
        )
        .unwrap();
        let planned = PlannedToolpath::from_toolpath(&path, 4.0, 2.0, 10.0).unwrap();
        let report = validate_planned_toolpath(&planned);
        assert!(report.is_structurally_valid(), "{:?}", report.issues);
        assert_eq!(
            planned
                .moves
                .iter()
                .filter(|x| x.kind == PlannedMoveKind::RapidTraverse)
                .count(),
            1
        );
        assert_eq!(report.cutting_moves, 0);
    }
    #[test]
    fn dangling_or_repeated_rapid_is_rejected() {
        let mut path = plan(RasterPattern::Zigzag);
        path.moves.truncate(6);
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::UnfinishedSequence
        ));
        let mut path = plan(RasterPattern::Zigzag);
        let duplicate = path.moves[5];
        path.moves.insert(6, duplicate);
        assert!(has(
            &validate_planned_toolpath(&path),
            IssueCode::WrongOrder
        ));
    }
}

//! P2-A: contrato canonico para la traza de un programa NC *interpretado*.
//!
//! No es una trayectoria CAM, no contiene un postprocesador ni analiza G-code.
//! Las futuras simulaciones consumiran los eventos reconstruidos al LEER el
//! G-code final. No deberan consumir directamente `PlannedToolpath`.
//!
//! Coordenadas: marco de trabajo explicito, XYZ absolutos en mm fisicos.
//! Los cambios G20/G21 y G90/G91 son eventos modales, nunca alteran el
//! significado de las coordenadas ya decodificadas. No se presuponen offsets
//! de maquina, posicion de arranque ni movimientos ocultos del controlador.

use std::fmt;

pub const MAX_NC_EVENTS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcUnits {
    Millimeters,
    Inches,
}

impl NcUnits {
    pub const fn mm_per_unit(self) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Inches => 25.4,
        }
    }

    /// Convierte una magnitud leida de G-code a mm. No redondea a la
    /// precision de salida del postprocesador.
    pub fn to_mm(self, value: f64) -> Result<f64, NcContractError> {
        let result = value * self.mm_per_unit();
        if !value.is_finite() || !result.is_finite() {
            return Err(NcContractError::InvalidNumber);
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcDistanceMode {
    Absolute,
    Incremental,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcMotionKind {
    Rapid,
    LinearFeed,
    // G2/G3 requieren plano, centro/radio, barrido y sentido explicitos.
    // Se incorporaran en otra variante, nunca degradados a G1 silenciosamente.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcSpindleMode {
    Stopped,
    Clockwise,
    CounterClockwise,
}

/// Posicion inicial proporcionada de manera explicita por el llamante.
/// No se infiere el origen maquina ni se supone que la herramienta esta en 0,0,0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NcInitialState {
    pub position_work_mm: [f64; 3],
    pub units: NcUnits,
    pub distance_mode: NcDistanceMode,
    pub feed_mm_min: Option<f64>,
    pub spindle_rpm: Option<f64>,
    pub spindle_mode: NcSpindleMode,
    pub tool_number: Option<u32>,
}

/// Movimiento REALMENTE programado, despues de interpretar estados modales.
/// La ausencia de F en G0 es intencional: la velocidad rapida depende de maquina.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NcMotion {
    pub kind: NcMotionKind,
    pub from_work_mm: [f64; 3],
    pub to_work_mm: [f64; 3],
    pub effective_feed_mm_min: Option<f64>,
}

/// Los eventos preservan el orden del programa. No se atribuye la condicion
/// 'cortando' a G1 sin evaluar el volumen de material y la herramienta activa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NcEventKind {
    Units(NcUnits),
    DistanceMode(NcDistanceMode),
    FeedRateMmMin(f64),
    SpindleSpeedRpm(f64),
    Spindle(NcSpindleMode),
    SelectTool(u32),
    ToolChange(u32),
    Motion(NcMotion),
    ProgramEnd,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NcEvent {
    /// Linea fisica del G-code final, empezando en 1.
    pub source_line: usize,
    pub kind: NcEventKind,
}

/// Traza modal completa en el marco de trabajo. No representa ejes maquina,
/// G53, desplazamientos G54..G59 ni ciclos que el interprete no soporte.
#[derive(Clone, Debug, PartialEq)]
pub struct NcTrace {
    pub initial: NcInitialState,
    pub events: Vec<NcEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcContractError {
    InvalidNumber,
    InvalidInitialState,
    TooManyEvents,
    InvalidSourceLine,
    InvalidCoordinates,
    InvalidFeed,
    InvalidSpindleSpeed,
    SpindleSpeedNotSet,
    RapidWithFeed,
    LinearWithoutFeed,
    InconsistentModalFeed,
    ZeroLengthMotion,
    DiscontinuousMotion,
    EventAfterProgramEnd,
    MissingProgramEnd,
}

impl fmt::Display for NcContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Contrato de traza NC invalido: {self:?}")
    }
}
impl std::error::Error for NcContractError {}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn valid_position(p: [f64; 3]) -> bool {
    p.iter().all(|v| v.is_finite())
}

impl NcTrace {
    /// Valida la continuidad y el estado modal sin suponer que la secuencia
    /// sea segura para ejecutar en una CNC. Ninguna llamada escribe el STL.
    pub fn validate(&self) -> Result<(), NcContractError> {
        if !valid_position(self.initial.position_work_mm) {
            return Err(NcContractError::InvalidInitialState);
        }
        if self.initial.feed_mm_min.is_some_and(|x| !valid_positive(x)) {
            return Err(NcContractError::InvalidFeed);
        }
        if self.initial.spindle_rpm.is_some_and(|x| !valid_positive(x)) {
            return Err(NcContractError::InvalidSpindleSpeed);
        }
        if self.initial.spindle_mode != NcSpindleMode::Stopped && self.initial.spindle_rpm.is_none()
        {
            return Err(NcContractError::SpindleSpeedNotSet);
        }
        if self.events.len() > MAX_NC_EVENTS {
            return Err(NcContractError::TooManyEvents);
        }
        let mut position = self.initial.position_work_mm;
        let mut feed = self.initial.feed_mm_min;
        let mut spindle_rpm = self.initial.spindle_rpm;
        let mut line = 0usize;
        let mut ended = false;
        for event in &self.events {
            if ended {
                return Err(NcContractError::EventAfterProgramEnd);
            }
            if event.source_line == 0 || event.source_line < line {
                return Err(NcContractError::InvalidSourceLine);
            }
            line = event.source_line;
            match event.kind {
                NcEventKind::FeedRateMmMin(value) => {
                    if !valid_positive(value) {
                        return Err(NcContractError::InvalidFeed);
                    }
                    feed = Some(value);
                }
                NcEventKind::SpindleSpeedRpm(value) => {
                    if !valid_positive(value) {
                        return Err(NcContractError::InvalidSpindleSpeed);
                    }
                    spindle_rpm = Some(value);
                }
                NcEventKind::Spindle(mode) => {
                    if mode != NcSpindleMode::Stopped && spindle_rpm.is_none() {
                        return Err(NcContractError::SpindleSpeedNotSet);
                    }
                }
                NcEventKind::Motion(motion) => {
                    if !valid_position(motion.from_work_mm) || !valid_position(motion.to_work_mm) {
                        return Err(NcContractError::InvalidCoordinates);
                    }
                    if motion.from_work_mm != position {
                        return Err(NcContractError::DiscontinuousMotion);
                    }
                    if motion.from_work_mm == motion.to_work_mm {
                        return Err(NcContractError::ZeroLengthMotion);
                    }
                    match motion.kind {
                        NcMotionKind::Rapid => {
                            if motion.effective_feed_mm_min.is_some() {
                                return Err(NcContractError::RapidWithFeed);
                            }
                        }
                        NcMotionKind::LinearFeed => {
                            let value = motion
                                .effective_feed_mm_min
                                .ok_or(NcContractError::LinearWithoutFeed)?;
                            if !valid_positive(value) {
                                return Err(NcContractError::InvalidFeed);
                            }
                            if feed != Some(value) {
                                return Err(NcContractError::InconsistentModalFeed);
                            }
                        }
                    }
                    position = motion.to_work_mm;
                }
                NcEventKind::ProgramEnd => ended = true,
                NcEventKind::Units(_)
                | NcEventKind::DistanceMode(_)
                | NcEventKind::SelectTool(_)
                | NcEventKind::ToolChange(_) => {}
            }
        }
        if !ended {
            return Err(NcContractError::MissingProgramEnd);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> NcInitialState {
        NcInitialState {
            position_work_mm: [0.0, 0.0, 10.0],
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        }
    }
    fn event(source_line: usize, kind: NcEventKind) -> NcEvent {
        NcEvent { source_line, kind }
    }
    fn sample() -> NcTrace {
        NcTrace {
            initial: start(),
            events: vec![
                event(1, NcEventKind::Units(NcUnits::Millimeters)),
                event(2, NcEventKind::FeedRateMmMin(300.0)),
                event(3, NcEventKind::SpindleSpeedRpm(12000.0)),
                event(4, NcEventKind::Spindle(NcSpindleMode::Clockwise)),
                event(
                    5,
                    NcEventKind::Motion(NcMotion {
                        kind: NcMotionKind::LinearFeed,
                        from_work_mm: [0.0, 0.0, 10.0],
                        to_work_mm: [2.0, 0.0, 10.0],
                        effective_feed_mm_min: Some(300.0),
                    }),
                ),
                event(
                    6,
                    NcEventKind::Motion(NcMotion {
                        kind: NcMotionKind::Rapid,
                        from_work_mm: [2.0, 0.0, 10.0],
                        to_work_mm: [2.0, 3.0, 10.0],
                        effective_feed_mm_min: None,
                    }),
                ),
                event(7, NcEventKind::Spindle(NcSpindleMode::Stopped)),
                event(8, NcEventKind::ProgramEnd),
            ],
        }
    }

    #[test]
    fn metric_and_inch_conversion_are_physical_not_display() {
        assert_eq!(NcUnits::Inches.to_mm(1.0), Ok(25.4));
        assert_eq!(NcUnits::Inches.to_mm(10.0), Ok(254.0));
        assert_eq!(NcUnits::Millimeters.to_mm(25.4), Ok(25.4));
        assert_eq!(
            NcUnits::Inches.to_mm(f64::INFINITY),
            Err(NcContractError::InvalidNumber)
        );
        assert_eq!(
            NcUnits::Inches.to_mm(f64::MAX),
            Err(NcContractError::InvalidNumber)
        );
    }

    #[test]
    fn sample_trace_is_continuous_and_has_explicit_end() {
        assert_eq!(sample().validate(), Ok(()));
    }

    #[test]
    fn initial_pose_is_never_invented() {
        let mut t = sample();
        t.initial.position_work_mm = [0.0, 0.0, f64::NAN];
        assert_eq!(t.validate(), Err(NcContractError::InvalidInitialState));
        let mut t = sample();
        t.initial.position_work_mm = [1.0, 0.0, 10.0];
        assert_eq!(t.validate(), Err(NcContractError::DiscontinuousMotion));
    }

    #[test]
    fn rapid_feed_and_linear_without_feed_are_rejected() {
        let mut t = sample();
        if let NcEventKind::Motion(ref mut m) = t.events[5].kind {
            m.effective_feed_mm_min = Some(200.0);
        }
        assert_eq!(t.validate(), Err(NcContractError::RapidWithFeed));
        let mut t = sample();
        if let NcEventKind::Motion(ref mut m) = t.events[4].kind {
            m.effective_feed_mm_min = None;
        }
        assert_eq!(t.validate(), Err(NcContractError::LinearWithoutFeed));
    }

    #[test]
    fn modal_feed_must_match_interpreted_motion() {
        let mut t = sample();
        if let NcEventKind::Motion(ref mut m) = t.events[4].kind {
            m.effective_feed_mm_min = Some(301.0);
        }
        assert_eq!(t.validate(), Err(NcContractError::InconsistentModalFeed));
    }

    #[test]
    fn event_order_and_source_lines_are_enforced() {
        let mut t = sample();
        t.events[2].source_line = 0;
        assert_eq!(t.validate(), Err(NcContractError::InvalidSourceLine));
        let mut t = sample();
        t.events[2].source_line = 1;
        assert_eq!(t.validate(), Err(NcContractError::InvalidSourceLine));
        let mut t = sample();
        t.events.push(event(9, NcEventKind::SelectTool(1)));
        assert_eq!(t.validate(), Err(NcContractError::EventAfterProgramEnd));
    }

    #[test]
    fn program_end_is_required_and_tool_events_are_preserved() {
        let mut t = sample();
        t.events.pop();
        assert_eq!(t.validate(), Err(NcContractError::MissingProgramEnd));
        let mut t = sample();
        t.events.insert(0, event(1, NcEventKind::SelectTool(2)));
        t.events.insert(1, event(1, NcEventKind::ToolChange(2)));
        assert_eq!(t.validate(), Ok(()));
    }

    #[test]
    fn spindle_must_have_a_configured_speed() {
        let mut t = sample();
        t.events.remove(2);
        assert_eq!(t.validate(), Err(NcContractError::SpindleSpeedNotSet));
    }

    #[test]
    fn units_and_distance_modes_do_not_rewrite_decoded_xyz() {
        let mut t = sample();
        t.events
            .insert(1, event(1, NcEventKind::Units(NcUnits::Inches)));
        t.events.insert(
            2,
            event(1, NcEventKind::DistanceMode(NcDistanceMode::Incremental)),
        );
        assert_eq!(t.validate(), Ok(()));
        if let NcEventKind::Motion(m) = t.events[6].kind {
            assert_eq!(m.to_work_mm, [2.0, 0.0, 10.0]);
        } else {
            panic!("Se esperaba movimiento canonico");
        }
    }
}

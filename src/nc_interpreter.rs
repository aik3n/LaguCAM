//! P2-C1: interprete NC estricto, independiente de egui, STL y postprocesador.
//!
//! Interpreta EL TEXTO FINAL, incluidos Start/End, comentarios y numeracion N.
//! Entrega la traza canonica `NcTrace` en mm y coordenadas absolutas de trabajo.
//! El origen/posicion fisica ANTES de Start es una entrada obligatoria: nunca
//! se inventan un home, un cero de maquina ni un movimiento anterior al NC.
//!
//! Subconjunto inicial: G0/G1, G17, G20/G21, G90/G91, G94, F, S, T,
//! M3/M4/M5 y M2/M30. Se admite G-code sin espacios (G91G1Z1F10).
//! Se rechazan expresamente los codigos de desplazamiento de coordenadas,
//! ciclos, compensaciones, arcos, G53, G54..G59, G92 y comandos desconocidos.
//! Esta traza NO es una certificacion de seguridad ni un modelo de aceleraciones.

use crate::nc_contract::{
    MAX_NC_EVENTS, NcContractError, NcDistanceMode, NcEvent, NcEventKind, NcInitialState, NcMotion,
    NcMotionKind, NcSpindleMode, NcTrace, NcUnits,
};
use std::fmt;

const MAX_NC_TEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_NC_LINE_BYTES: usize = 2048;
const MAX_WORDS_PER_LINE: usize = 64;
const MAX_NUMBER_BYTES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NcInterpretErrorKind {
    InputTooLarge,
    LineTooLong,
    TooManyWords,
    TooManyEvents,
    UnexpectedCharacter,
    UnclosedComment,
    NestedComment,
    MissingNumber,
    InvalidNumber,
    UnsupportedWord(char),
    UnsupportedGCode(u32),
    UnsupportedMCode(u32),
    InvalidCodeNumber,
    DuplicateWord(char),
    ConflictingModalCodes,
    MotionModeNotSet,
    ProgramEnded,
    EndWithAxisWords,
    MissingProgramEnd,
    CoordinateOverflow,
    IncrementLostToPrecision,
    Contract(NcContractError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NcInterpretError {
    /// Linea fisica del archivo NC, comenzando en 1.
    pub line: usize,
    pub kind: NcInterpretErrorKind,
}

impl fmt::Display for NcInterpretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "G-code no interpretable en linea {}: {:?}",
            self.line, self.kind
        )
    }
}
impl std::error::Error for NcInterpretError {}

fn error(line: usize, kind: NcInterpretErrorKind) -> NcInterpretError {
    NcInterpretError { line, kind }
}

#[derive(Clone, Copy)]
struct Word {
    letter: char,
    value: f64,
}

fn scan_words(line: &str, source_line: usize) -> Result<Vec<Word>, NcInterpretError> {
    if line.len() > MAX_NC_LINE_BYTES {
        return Err(error(source_line, NcInterpretErrorKind::LineTooLong));
    }
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut result = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' => i += 1,
            b';' => break, // Comentario hasta el final de la linea.
            b'(' => {
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b')' => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        b'(' => {
                            return Err(error(source_line, NcInterpretErrorKind::NestedComment));
                        }
                        _ => i += 1,
                    }
                }
                if !closed {
                    return Err(error(source_line, NcInterpretErrorKind::UnclosedComment));
                }
            }
            b'A'..=b'Z' | b'a'..=b'z' => {
                let letter = (bytes[i] as char).to_ascii_uppercase();
                i += 1;
                let start = i;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                let mut digits = 0;
                let mut dot = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'0'..=b'9' => {
                            digits += 1;
                            i += 1;
                        }
                        b'.' if !dot => {
                            dot = true;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                if digits == 0 {
                    return Err(error(source_line, NcInterpretErrorKind::MissingNumber));
                }
                if i - start > MAX_NUMBER_BYTES {
                    return Err(error(source_line, NcInterpretErrorKind::InvalidNumber));
                }
                let raw = std::str::from_utf8(&bytes[start..i])
                    .map_err(|_| error(source_line, NcInterpretErrorKind::InvalidNumber))?;
                let value: f64 = raw
                    .parse()
                    .map_err(|_| error(source_line, NcInterpretErrorKind::InvalidNumber))?;
                if !value.is_finite() {
                    return Err(error(source_line, NcInterpretErrorKind::InvalidNumber));
                }
                if result.len() == MAX_WORDS_PER_LINE {
                    return Err(error(source_line, NcInterpretErrorKind::TooManyWords));
                }
                result.push(Word { letter, value });
            }
            _ => {
                return Err(error(
                    source_line,
                    NcInterpretErrorKind::UnexpectedCharacter,
                ));
            }
        }
    }
    Ok(result)
}

fn code_as_u32(value: f64, line: usize) -> Result<u32, NcInterpretError> {
    if value < 0.0 || value > f64::from(u32::MAX) || value.fract() != 0.0 {
        return Err(error(line, NcInterpretErrorKind::InvalidCodeNumber));
    }
    Ok(value as u32)
}

fn emit(events: &mut Vec<NcEvent>, line: usize, kind: NcEventKind) -> Result<(), NcInterpretError> {
    if events.len() >= MAX_NC_EVENTS {
        return Err(error(line, NcInterpretErrorKind::TooManyEvents));
    }
    events.push(NcEvent {
        source_line: line,
        kind,
    });
    Ok(())
}

#[derive(Clone, Copy)]
struct State {
    units: NcUnits,
    distance: NcDistanceMode,
    motion: Option<NcMotionKind>,
    position: [f64; 3],
    feed: Option<f64>,
    spindle_rpm: Option<f64>,
    ended: bool,
}

impl State {
    fn from_initial(initial: NcInitialState) -> Self {
        Self {
            units: initial.units,
            distance: initial.distance_mode,
            motion: None,
            position: initial.position_work_mm,
            feed: initial.feed_mm_min,
            spindle_rpm: initial.spindle_rpm,
            ended: false,
        }
    }
}

/// Interpreta codigo NC como instrucciones modales; no lee el CAM de origen.
/// `initial` describe el estado ANTES de ejecutar la primera linea, incluida
/// Start. Debe obtenerse de una precondicion explicita, nunca del destino del
/// primer movimiento o de un valor 0,0,0 supuesto.
///
/// La ausencia de soporte de una instruccion termina con error y numero de
/// linea. Solo un resultado `Ok` produce una `NcTrace` completa y validada.
pub fn interpret_program(text: &str, initial: NcInitialState) -> Result<NcTrace, NcInterpretError> {
    if text.len() > MAX_NC_TEXT_BYTES {
        return Err(error(0, NcInterpretErrorKind::InputTooLarge));
    }
    // El estado inicial se verifica antes de intentar consumir un solo bloque.
    // La validacion de NcTrace se hace otra vez al final para garantizar que
    // todos los movimientos cumplen su contrato publico.
    let seed = NcTrace {
        initial,
        events: vec![NcEvent {
            source_line: 1,
            kind: NcEventKind::ProgramEnd,
        }],
    };
    seed.validate()
        .map_err(|e| error(0, NcInterpretErrorKind::Contract(e)))?;

    let mut state = State::from_initial(initial);
    let mut events = Vec::new();
    for (i, physical_line) in text.split('\n').enumerate() {
        let line = i + 1;
        let words = scan_words(physical_line, line)?;
        if words.is_empty() {
            continue;
        }
        if state.ended {
            return Err(error(line, NcInterpretErrorKind::ProgramEnded));
        }
        let mut axes = [None::<f64>; 3];
        let mut new_units = None;
        let mut new_distance = None;
        let mut new_motion = None;
        let mut new_feed = None;
        let mut new_rpm = None;
        let mut new_spindle = None;
        let mut new_tool = None;
        let mut end = false;
        let mut has_n = false;
        let mut has_plane = false;
        let mut has_feed_mode = false;
        for (index, word) in words.iter().enumerate() {
            let value = word.value;
            match word.letter {
                'N' => {
                    if index != 0 || has_n {
                        return Err(error(line, NcInterpretErrorKind::DuplicateWord('N')));
                    }
                    code_as_u32(value, line)?;
                    has_n = true;
                }
                'X' | 'Y' | 'Z' => {
                    let axis = match word.letter {
                        'X' => 0,
                        'Y' => 1,
                        _ => 2,
                    };
                    if axes[axis].replace(value).is_some() {
                        return Err(error(
                            line,
                            NcInterpretErrorKind::DuplicateWord(word.letter),
                        ));
                    }
                }
                'F' => {
                    if new_feed.replace(value).is_some() {
                        return Err(error(line, NcInterpretErrorKind::DuplicateWord('F')));
                    }
                }
                'S' => {
                    if new_rpm.replace(value).is_some() {
                        return Err(error(line, NcInterpretErrorKind::DuplicateWord('S')));
                    }
                }
                'T' => {
                    let number = code_as_u32(value, line)?;
                    if new_tool.replace(number).is_some() {
                        return Err(error(line, NcInterpretErrorKind::DuplicateWord('T')));
                    }
                }
                'G' => match code_as_u32(value, line)? {
                    0 | 1 => {
                        let motion = if value == 0.0 {
                            NcMotionKind::Rapid
                        } else {
                            NcMotionKind::LinearFeed
                        };
                        if new_motion.replace(motion).is_some() {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                    }
                    17 => {
                        if has_plane {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                        has_plane = true;
                    }
                    20 | 21 => {
                        let units = if value == 20.0 {
                            NcUnits::Inches
                        } else {
                            NcUnits::Millimeters
                        };
                        if new_units.replace(units).is_some() {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                    }
                    90 | 91 => {
                        let mode = if value == 90.0 {
                            NcDistanceMode::Absolute
                        } else {
                            NcDistanceMode::Incremental
                        };
                        if new_distance.replace(mode).is_some() {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                    }
                    94 => {
                        if has_feed_mode {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                        has_feed_mode = true;
                    }
                    other => {
                        return Err(error(line, NcInterpretErrorKind::UnsupportedGCode(other)));
                    }
                },
                'M' => match code_as_u32(value, line)? {
                    3..=5 => {
                        let mode = match value as u32 {
                            3 => NcSpindleMode::Clockwise,
                            4 => NcSpindleMode::CounterClockwise,
                            _ => NcSpindleMode::Stopped,
                        };
                        if new_spindle.replace(mode).is_some() {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                    }
                    2 | 30 => {
                        if end {
                            return Err(error(line, NcInterpretErrorKind::ConflictingModalCodes));
                        }
                        end = true;
                    }
                    other => {
                        return Err(error(line, NcInterpretErrorKind::UnsupportedMCode(other)));
                    }
                },
                other => return Err(error(line, NcInterpretErrorKind::UnsupportedWord(other))),
            }
        }
        if end && axes.iter().any(Option::is_some) {
            return Err(error(line, NcInterpretErrorKind::EndWithAxisWords));
        }
        // Todos los modos del bloque se aplican antes de interpretar XYZ y F.
        if let Some(units) = new_units {
            state.units = units;
            emit(&mut events, line, NcEventKind::Units(units))?;
        }
        if let Some(mode) = new_distance {
            state.distance = mode;
            emit(&mut events, line, NcEventKind::DistanceMode(mode))?;
        }
        if let Some(kind) = new_motion {
            state.motion = Some(kind);
        }
        if let Some(value) = new_feed {
            let physical = state
                .units
                .to_mm(value)
                .map_err(|e| error(line, NcInterpretErrorKind::Contract(e)))?;
            if physical <= 0.0 {
                return Err(error(
                    line,
                    NcInterpretErrorKind::Contract(NcContractError::InvalidFeed),
                ));
            }
            state.feed = Some(physical);
            emit(&mut events, line, NcEventKind::FeedRateMmMin(physical))?;
        }
        if let Some(rpm) = new_rpm {
            if rpm <= 0.0 {
                return Err(error(
                    line,
                    NcInterpretErrorKind::Contract(NcContractError::InvalidSpindleSpeed),
                ));
            }
            state.spindle_rpm = Some(rpm);
            emit(&mut events, line, NcEventKind::SpindleSpeedRpm(rpm))?;
        }
        if let Some(tool) = new_tool {
            emit(&mut events, line, NcEventKind::SelectTool(tool))?;
        }
        if let Some(spindle) = new_spindle {
            if spindle != NcSpindleMode::Stopped && state.spindle_rpm.is_none() {
                return Err(error(
                    line,
                    NcInterpretErrorKind::Contract(NcContractError::SpindleSpeedNotSet),
                ));
            }
            emit(&mut events, line, NcEventKind::Spindle(spindle))?;
        }
        if axes.iter().any(Option::is_some) {
            let motion = state
                .motion
                .ok_or_else(|| error(line, NcInterpretErrorKind::MotionModeNotSet))?;
            let from = state.position;
            let mut to = from;
            for axis in 0..3 {
                if let Some(value) = axes[axis] {
                    let mm = state
                        .units
                        .to_mm(value)
                        .map_err(|e| error(line, NcInterpretErrorKind::Contract(e)))?;
                    to[axis] = match state.distance {
                        NcDistanceMode::Absolute => mm,
                        NcDistanceMode::Incremental => {
                            let added = from[axis] + mm;
                            if mm != 0.0 && added == from[axis] {
                                return Err(error(
                                    line,
                                    NcInterpretErrorKind::IncrementLostToPrecision,
                                ));
                            }
                            added
                        }
                    };
                    if !to[axis].is_finite() {
                        return Err(error(line, NcInterpretErrorKind::CoordinateOverflow));
                    }
                }
            }
            // Una orden modal con XYZ identicas no crea desplazamiento fisico.
            // No se introduce un falso evento de longitud cero en NcTrace.
            if to != from {
                if motion == NcMotionKind::LinearFeed && state.feed.is_none() {
                    return Err(error(
                        line,
                        NcInterpretErrorKind::Contract(NcContractError::LinearWithoutFeed),
                    ));
                }
                emit(
                    &mut events,
                    line,
                    NcEventKind::Motion(NcMotion {
                        kind: motion,
                        from_work_mm: from,
                        to_work_mm: to,
                        effective_feed_mm_min: if motion == NcMotionKind::Rapid {
                            None
                        } else {
                            state.feed
                        },
                    }),
                )?;
                state.position = to;
            }
        }
        if end {
            emit(&mut events, line, NcEventKind::ProgramEnd)?;
            state.ended = true;
        }
    }
    if !state.ended {
        return Err(error(
            text.lines().count().max(1),
            NcInterpretErrorKind::MissingProgramEnd,
        ));
    }
    let trace = NcTrace { initial, events };
    trace.validate().map_err(|e| {
        let line = trace.events.last().map_or(0, |e| e.source_line);
        error(line, NcInterpretErrorKind::Contract(e))
    })?;
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam_contract::ConfiguredCuttingConditions;
    use crate::nc_post::{PostInput, PostMove, PostProcessorProfile, PostUnits, generate_program};
    use crate::toolpath_links::PlannedMoveKind;

    fn initial(p: [f64; 3]) -> NcInitialState {
        NcInitialState {
            position_work_mm: p,
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        }
    }
    fn motions(trace: &NcTrace) -> Vec<NcMotion> {
        trace
            .events
            .iter()
            .filter_map(|e| {
                if let NcEventKind::Motion(m) = e.kind {
                    Some(m)
                } else {
                    None
                }
            })
            .collect()
    }
    fn kind(result: Result<NcTrace, NcInterpretError>) -> NcInterpretErrorKind {
        result
            .expect_err("el archivo no debe ser interpretable")
            .kind
    }

    #[test]
    fn generated_metric_nc_is_interpreted_from_text_not_cam() {
        let path = [
            PostMove {
                kind: PlannedMoveKind::RapidTraverse,
                from_work_mm: [1.0, 2.0, 10.0],
                to_work_mm: [5.0, 2.0, 10.0],
                pass_index: 0,
            },
            PostMove {
                kind: PlannedMoveKind::Cutting,
                from_work_mm: [5.0, 2.0, 10.0],
                to_work_mm: [5.0, 2.0, 3.0],
                pass_index: 0,
            },
        ];
        let program = generate_program(
            &PostProcessorProfile::grbl_3axis_mm(),
            PostInput {
                moves_work_mm: &path,
                conditions: ConfiguredCuttingConditions {
                    feed_mm_min: 500.0,
                    spindle_rpm: 8000.0,
                },
            },
        )
        .unwrap();
        let trace = interpret_program(
            &program.text,
            initial(program.required_first_move_position_work_mm),
        )
        .unwrap();
        let movements = motions(&trace);
        assert_eq!(movements.len(), 2);
        assert_eq!(movements[0].kind, NcMotionKind::Rapid);
        assert_eq!(movements[0].from_work_mm, [1.0, 2.0, 10.0]);
        assert_eq!(movements[0].to_work_mm, [5.0, 2.0, 10.0]);
        assert_eq!(movements[1].to_work_mm, [5.0, 2.0, 3.0]);
        assert_eq!(movements[1].effective_feed_mm_min, Some(500.0));
        assert_eq!(movements[0].effective_feed_mm_min, None);
        assert!(matches!(
            trace.events.last().unwrap().kind,
            NcEventKind::ProgramEnd
        ));
    }

    #[test]
    fn generated_inches_use_physical_mm_and_conversion_of_feed() {
        let path = [PostMove {
            kind: PlannedMoveKind::Cutting,
            from_work_mm: [0.0, 0.0, 25.4],
            to_work_mm: [25.4, 0.0, 25.4],
            pass_index: 0,
        }];
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.units = PostUnits::Inches;
        let generated = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: ConfiguredCuttingConditions {
                    feed_mm_min: 254.0,
                    spindle_rpm: 14000.0,
                },
            },
        )
        .unwrap();
        let trace = interpret_program(
            &generated.text,
            initial(generated.required_first_move_position_work_mm),
        )
        .unwrap();
        let movement = motions(&trace)[0];
        assert_eq!(movement.to_work_mm, [25.4, 0.0, 25.4]);
        assert_eq!(movement.effective_feed_mm_min, Some(254.0));
        assert!(
            trace
                .events
                .iter()
                .any(|e| e.kind == NcEventKind::Units(NcUnits::Inches))
        );
    }

    #[test]
    fn custom_start_incremental_move_is_real_motion_before_generated_cam() {
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.start_commands = "(inicio)\nG91G1Z1F10".into();
        let path = [PostMove {
            kind: PlannedMoveKind::Cutting,
            from_work_mm: [0.0, 0.0, 1.0],
            to_work_mm: [2.0, 0.0, 1.0],
            pass_index: 0,
        }];
        let generated = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: ConfiguredCuttingConditions {
                    feed_mm_min: 300.0,
                    spindle_rpm: 12000.0,
                },
            },
        )
        .unwrap();
        let trace = interpret_program(&generated.text, initial([0.0, 0.0, 0.0])).unwrap();
        let movement = motions(&trace);
        assert_eq!(movement.len(), 2);
        assert_eq!(movement[0].to_work_mm, [0.0, 0.0, 1.0]);
        assert_eq!(movement[0].effective_feed_mm_min, Some(10.0));
        assert_eq!(movement[1].from_work_mm, [0.0, 0.0, 1.0]);
        assert_eq!(movement[1].to_work_mm, [2.0, 0.0, 1.0]);
        assert_eq!(movement[1].effective_feed_mm_min, Some(300.0));
    }

    #[test]
    fn comments_and_line_numbers_do_not_change_motions() {
        let text = "N5 (inicio)\r\nN10 G21 G90 G94 G17 ; comentario\r\nN15 S8000M3\r\nN20 G1X1Y2Z3F500 (corte)\r\nN25 G1X2 ; corte\r\nN30 M5M30\r\n";
        let trace = interpret_program(text, initial([0.0, 0.0, 0.0])).unwrap();
        assert_eq!(motions(&trace).len(), 2);
        assert_eq!(motions(&trace)[0].to_work_mm, [1.0, 2.0, 3.0]);
        assert_eq!(motions(&trace)[1].to_work_mm, [2.0, 2.0, 3.0]);
        assert_eq!(motions(&trace)[0].effective_feed_mm_min, Some(500.0));
        assert_eq!(trace.events.last().unwrap().source_line, 6);
    }

    #[test]
    fn modal_modes_and_partial_axes_affect_physical_motion() {
        let text = "G21\nG91G1X1F100\nY2\nG90G0Z10\nG20\nG1 X1 F10\nM30\n";
        let trace = interpret_program(text, initial([0.0, 0.0, 0.0])).unwrap();
        let m = motions(&trace);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].to_work_mm, [1.0, 0.0, 0.0]);
        assert_eq!(m[1].to_work_mm, [1.0, 2.0, 0.0]);
        assert_eq!(m[2].to_work_mm, [1.0, 2.0, 10.0]);
        assert_eq!(m[3].to_work_mm, [25.4, 2.0, 10.0]);
        assert_eq!(m[3].effective_feed_mm_min, Some(254.0));
    }

    #[test]
    fn unsupported_codes_are_never_silently_ignored() {
        for (code, expected) in [
            ("G54\nM30", NcInterpretErrorKind::UnsupportedGCode(54)),
            ("G92X1\nM30", NcInterpretErrorKind::UnsupportedGCode(92)),
            ("G2X2Y0I1J0\nM30", NcInterpretErrorKind::UnsupportedGCode(2)),
            ("M8\nM30", NcInterpretErrorKind::UnsupportedMCode(8)),
            ("M6\nM30", NcInterpretErrorKind::UnsupportedMCode(6)),
            ("G53G0X0\nM30", NcInterpretErrorKind::UnsupportedGCode(53)),
            ("G93\nM30", NcInterpretErrorKind::UnsupportedGCode(93)),
            ("G18\nM30", NcInterpretErrorKind::UnsupportedGCode(18)),
        ] {
            assert_eq!(
                kind(interpret_program(code, initial([0.0; 3]))),
                expected,
                "{code}"
            );
        }
    }

    #[test]
    fn rejects_unknown_initial_motion_mode_and_missing_feed() {
        assert_eq!(
            kind(interpret_program("X1\nM30", initial([0.0; 3]))),
            NcInterpretErrorKind::MotionModeNotSet
        );
        assert_eq!(
            kind(interpret_program("G1 X1\nM30", initial([0.0; 3]))),
            NcInterpretErrorKind::Contract(NcContractError::LinearWithoutFeed)
        );
        assert_eq!(
            kind(interpret_program("G1 X1 F0\nM30", initial([0.0; 3]))),
            NcInterpretErrorKind::Contract(NcContractError::InvalidFeed)
        );
        assert_eq!(
            kind(interpret_program("M3\nM30", initial([0.0; 3]))),
            NcInterpretErrorKind::Contract(NcContractError::SpindleSpeedNotSet)
        );
    }

    #[test]
    fn invalid_and_ambiguous_programs_fail_closed() {
        let items = [
            "G0X1",                  // Sin fin de programa.
            "M30\nG0X1",             // Instruccion posterior al fin.
            "G0G1X1\nM30",           // Grupo modal incompatible.
            "G20G21\nM30",           // Unidades contradictorias.
            "G1X1X2F100\nM30",       // Dos X en un bloque.
            "G0X1 (comentario\nM30", // Comentario no cerrado.
            "G0X1 (a(b))\nM30",      // Comentarios anidados.
            "G1X1e3F100\nM30",       // Notacion exponental no soportada.
            "G90.1\nM30",            // Modos con decimales sin implementar.
            "G0X1 M30",              // Fin y movimiento en mismo bloque.
            "G0 X1 #2\nM30",         // Expresiones/macros no implementadas.
        ];
        for input in items {
            assert!(
                interpret_program(input, initial([0.0; 3])).is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn generated_numbered_crlf_program_roundtrips_as_nc() {
        use crate::nc_post::{PostLineEnding, PostLineNumbers};
        let path = [PostMove {
            kind: PlannedMoveKind::Cutting,
            from_work_mm: [0.0, 0.0, 3.0],
            to_work_mm: [3.23456, 0.0, 3.0],
            pass_index: 0,
        }];
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.line_numbers = Some(PostLineNumbers { start: 5, step: 5 });
        profile.line_ending = PostLineEnding::CrLf;
        let nc = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: ConfiguredCuttingConditions {
                    feed_mm_min: 600.0,
                    spindle_rpm: 12000.0,
                },
            },
        )
        .unwrap();
        assert!(nc.text.contains("\r\n"));
        let trace =
            interpret_program(&nc.text, initial(nc.required_first_move_position_work_mm)).unwrap();
        assert_eq!(motions(&trace)[0].to_work_mm, [3.235, 0.0, 3.0]);
        assert_eq!(motions(&trace)[0].from_work_mm, [0.0, 0.0, 3.0]);
    }

    #[test]
    fn unknown_command_reports_its_original_line_number() {
        let result = interpret_program("(inicio)\nG21\nG54\nM30", initial([0.0; 3]));
        assert_eq!(
            result.unwrap_err(),
            NcInterpretError {
                line: 3,
                kind: NcInterpretErrorKind::UnsupportedGCode(54),
            }
        );
    }

    #[test]
    fn zero_displacement_is_not_invented_as_motion() {
        let trace = interpret_program("G0 X0Y0Z0\nM30", initial([0.0; 3])).unwrap();
        assert!(motions(&trace).is_empty());
    }

    #[test]
    fn no_fake_initial_position_and_initial_feed_in_mm() {
        let mut invalid = initial([f64::NAN, 0.0, 0.0]);
        assert!(interpret_program("M30", invalid).is_err());
        invalid = initial([0.0; 3]);
        invalid.feed_mm_min = Some(10.0);
        let trace = interpret_program("G20\nG1 X1\nM30", invalid).unwrap();
        assert_eq!(motions(&trace)[0].effective_feed_mm_min, Some(10.0));
    }

    #[test]
    fn custom_end_motion_is_not_lost() {
        let trace =
            interpret_program("G21\nG90\nG1X2F100\nG91G1Z1F10\nM30", initial([0.0; 3])).unwrap();
        let m = motions(&trace);
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].from_work_mm, [2.0, 0.0, 0.0]);
        assert_eq!(m[1].to_work_mm, [2.0, 0.0, 1.0]);
    }
}

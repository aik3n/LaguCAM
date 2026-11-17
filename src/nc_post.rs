//! P2-B1: motor de postprocesado NC determinista, independiente de GUI y disco.
//!
//! Recibe movimientos ya convertidos a coordenadas DE TRABAJO en mm fisicos.
//! Nunca convierte directamente triangulos STL ni transforma el origen CNC.
//! Los bloques Start/End son texto libre del perfil y se conservan en orden;
//! pueden modificar estados modales y ejecutar movimientos. Este modulo NO
//! interpreta ese texto y nunca certifica su seguridad ni su simulacion.
//!
//! El primer movimiento exige que la herramienta ya este en su posicion FROM:
//! NO se inventa una aproximacion desde maquina ni un G0 de posicionamiento.
//! El programa devuelto debe interpretarse desde el texto NC final en P2-C,
//! incluyendo Start/End y los redondeos, antes de simular movimiento o material.

use crate::cam_contract::ConfiguredCuttingConditions;
use crate::nc_contract::NcUnits;
use crate::toolpath_links::PlannedMoveKind;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const POST_PROFILE_VERSION: u32 = 1;
pub const MAX_POST_MOVES: usize = 1_000_000;
const MAX_NC_BYTES: usize = 64 * 1024 * 1024;
const MAX_CUSTOM_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostUnits {
    Millimeters,
    Inches,
}

impl PostUnits {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Millimeters => "G21",
            Self::Inches => "G20",
        }
    }

    pub const fn as_nc_units(self) -> NcUnits {
        match self {
            Self::Millimeters => NcUnits::Millimeters,
            Self::Inches => NcUnits::Inches,
        }
    }

    fn mm_per_unit(self) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Inches => 25.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostSpindleDirection {
    Clockwise,
    CounterClockwise,
}

impl PostSpindleDirection {
    fn code(self) -> &'static str {
        match self {
            Self::Clockwise => "M3",
            Self::CounterClockwise => "M4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostLineEnding {
    Lf,
    CrLf,
}

impl PostLineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostLineNumbers {
    pub start: u32,
    pub step: u32,
}

/// Perfil versionado. La biblioteca de perfiles y su interfaz corresponden a
/// P2-B2; aqui no se crean archivos, ni se almacena el perfil en el proyecto.
/// Cada perfil determina sus unidades NC, independientemente de Options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostProcessorProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub controller: String,
    /// Extension sin punto ni ruta (ej. "nc").
    pub extension: String,
    pub units: PostUnits,
    pub xyz_decimals: u8,
    pub feed_decimals: u8,
    pub spindle_decimals: u8,
    pub suppress_unchanged_axes: bool,
    pub line_numbers: Option<PostLineNumbers>,
    pub line_ending: PostLineEnding,
    /// Texto G-code del usuario ANTES de la cabecera modal garantizada.
    pub start_commands: String,
    /// Texto G-code del usuario DESPUES de los movimientos CAM.
    pub end_commands: String,
    pub spindle_direction: PostSpindleDirection,
    pub auto_start_spindle: bool,
    pub auto_stop_spindle: bool,
    pub auto_program_end: bool,
}

impl PostProcessorProfile {
    /// Perfil de referencia GRBL 1.1, tres ejes: deliberadamente no incluye
    /// G53, G54, G28, G92 ni posicionamientos de maquina no configurados.
    pub fn grbl_3axis_mm() -> Self {
        Self {
            version: POST_PROFILE_VERSION,
            id: "grbl_3axis_mm_v1".into(),
            name: "GRBL 1.1 - tres ejes (mm)".into(),
            controller: "GRBL 1.1".into(),
            extension: "nc".into(),
            units: PostUnits::Millimeters,
            xyz_decimals: 3,
            feed_decimals: 3,
            spindle_decimals: 0,
            suppress_unchanged_axes: false,
            line_numbers: None,
            line_ending: PostLineEnding::Lf,
            start_commands: String::new(),
            end_commands: String::new(),
            spindle_direction: PostSpindleDirection::Clockwise,
            auto_start_spindle: true,
            auto_stop_spindle: true,
            auto_program_end: true,
        }
    }

    pub fn validate(&self) -> Result<(), PostError> {
        if self.version != POST_PROFILE_VERSION {
            return Err(PostError::UnsupportedProfileVersion);
        }
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.controller.trim().is_empty()
            || self.controller.len() > 128
            || self.name.chars().any(char::is_control)
            || self.controller.chars().any(char::is_control)
            || self.extension.is_empty()
            || self.extension.len() > 8
            || !self.extension.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(PostError::InvalidProfile);
        }
        if self.xyz_decimals > 8 || self.feed_decimals > 6 || self.spindle_decimals > 3 {
            return Err(PostError::InvalidPrecision);
        }
        if let Some(numbering) = self.line_numbers
            && (numbering.start == 0 || numbering.step == 0)
        {
            return Err(PostError::InvalidLineNumbers);
        }
        validate_user_block(&self.start_commands)?;
        validate_user_block(&self.end_commands)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostMove {
    pub kind: PlannedMoveKind,
    pub from_work_mm: [f64; 3],
    pub to_work_mm: [f64; 3],
    pub pass_index: usize,
}

/// Condiciones ya resueltas y una unica secuencia de herramienta, antes de
/// introducir un procedimiento explicito de cambios de herramienta en P2-B2.
#[derive(Clone, Copy, Debug)]
pub struct PostInput<'a> {
    pub moves_work_mm: &'a [PostMove],
    pub conditions: ConfiguredCuttingConditions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedNc {
    pub text: String,
    pub profile_id: String,
    pub units: NcUnits,
    /// Posicion exigida JUSTO ANTES del primer movimiento CAM emitido.
    /// No garantiza posicion inicial ANTES de ejecutar Start personalizado.
    pub required_first_move_position_work_mm: [f64; 3],
    pub last_cam_position_work_mm: [f64; 3],
    pub cam_move_count: usize,
    /// Siempre true: debe releerse el G-code definitivo para simularlo.
    pub requires_nc_interpretation: bool,
    pub has_custom_blocks: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostError {
    UnsupportedProfileVersion,
    InvalidProfile,
    InvalidPrecision,
    InvalidLineNumbers,
    InvalidCustomCommands,
    InvalidCuttingConditions,
    EmptyToolpath,
    TooManyMoves,
    InvalidCoordinates,
    DiscontinuousPath,
    ZeroLengthMotion,
    RoundedMotionLost,
    NumericRange,
    OutputTooLarge,
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::UnsupportedProfileVersion => "Version del perfil de postprocesador no soportada",
            Self::InvalidProfile => "Identidad o extension del perfil no valida",
            Self::InvalidPrecision => "Precision numerica NC fuera de rango",
            Self::InvalidLineNumbers => "Numeracion de lineas NC no valida",
            Self::InvalidCustomCommands => "Start/End contienen caracteres o lineas invalidos",
            Self::InvalidCuttingConditions => "Avance F o velocidad S no validos",
            Self::EmptyToolpath => "No hay movimientos CAM para postprocesar",
            Self::TooManyMoves => "Demasiados movimientos CAM para postprocesar",
            Self::InvalidCoordinates => "Movimiento CAM con coordenadas no finitas",
            Self::DiscontinuousPath => "Los movimientos CAM no son continuos",
            Self::ZeroLengthMotion => "Movimiento CAM de longitud nula",
            Self::RoundedMotionLost => "La precision NC hace desaparecer un movimiento CAM",
            Self::NumericRange => "Valor NC fuera de rango representable",
            Self::OutputTooLarge => "El programa NC supera el limite de salida",
        };
        f.write_str(label)
    }
}

impl std::error::Error for PostError {}

fn validate_user_block(code: &str) -> Result<(), PostError> {
    if code.len() > MAX_CUSTOM_BYTES {
        return Err(PostError::InvalidCustomCommands);
    }
    // Texto libre, pero sin caracteres de control que puedan ocultar lineas
    // o corromper la representacion (el tabulador esta permitido).
    if code
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(PostError::InvalidCustomCommands);
    }
    let normalized = code.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(PostError::InvalidCustomCommands);
    }
    let mut count = 0usize;
    for line in normalized.split('\n') {
        count += 1;
        if line.len() > 512 || count > 256 {
            return Err(PostError::InvalidCustomCommands);
        }
    }
    Ok(())
}

fn finite_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value <= 1_000_000.0
}

/// Redondea a la precision real emitida, sin suponer que el valor calculado
/// coincide exactamente con la coordenada legible por el controlador.
fn format_number(value: f64, decimals: u8) -> Result<String, PostError> {
    if !value.is_finite() || value.abs() > 1.0e12 {
        return Err(PostError::NumericRange);
    }
    let mut rendered = format!("{:.*}", usize::from(decimals), value);
    if rendered.len() > 48 {
        return Err(PostError::NumericRange);
    }
    if rendered.starts_with('-') && rendered.parse::<f64>().ok() == Some(0.0) {
        rendered.remove(0); // Evitar -0.000 en NC.
    }
    Ok(rendered)
}

fn formatted_position(
    p: [f64; 3],
    units: PostUnits,
    decimals: u8,
) -> Result<[String; 3], PostError> {
    let scale = units.mm_per_unit();
    let mut result = [String::new(), String::new(), String::new()];
    for i in 0..3 {
        if !p[i].is_finite() {
            return Err(PostError::InvalidCoordinates);
        }
        result[i] = format_number(p[i] / scale, decimals)?;
    }
    Ok(result)
}

struct NcWriter {
    text: String,
    line_ending: &'static str,
    next_line: Option<(u64, u64)>,
}

impl NcWriter {
    fn new(profile: &PostProcessorProfile) -> Self {
        Self {
            text: String::new(),
            line_ending: profile.line_ending.as_str(),
            next_line: profile
                .line_numbers
                .map(|n| (u64::from(n.start), u64::from(n.step))),
        }
    }

    fn line(&mut self, command: &str) -> Result<(), PostError> {
        let numbering = match self.next_line {
            Some((number, step)) => {
                if number > u64::from(u32::MAX) {
                    return Err(PostError::InvalidLineNumbers);
                }
                self.next_line = Some((number + step, step));
                format!("N{number} {command}")
            }
            None => command.to_owned(),
        };
        let bytes = numbering.len() + self.line_ending.len();
        if self
            .text
            .len()
            .checked_add(bytes)
            .is_none_or(|n| n > MAX_NC_BYTES)
        {
            return Err(PostError::OutputTooLarge);
        }
        self.text.push_str(&numbering);
        self.text.push_str(self.line_ending);
        Ok(())
    }

    fn custom(&mut self, block: &str) -> Result<(), PostError> {
        if block.is_empty() {
            return Ok(());
        }
        let normalized = block.replace("\r\n", "\n");
        for line in normalized.split_terminator('\n') {
            self.line(line)?;
        }
        Ok(())
    }
}

/// Genera un NC sin escribir en disco. NO verifica Start/End ni confirma
/// seguridad de maquina: el futuro interprete debera leer su texto completo.
/// No emite un rapido hasta la posicion del primer movimiento; esa posicion
/// es una precondicion fisica explicita para el futuro gestor de exportacion.
/// Los G20/G21, G90, G94 y G17 se restablecen DESPUES de Start para los
/// movimientos generados, pero los movimientos presentes en Start permanecen.
pub fn generate_program(
    profile: &PostProcessorProfile,
    input: PostInput<'_>,
) -> Result<GeneratedNc, PostError> {
    profile.validate()?;
    if !finite_positive(input.conditions.feed_mm_min)
        || !finite_positive(input.conditions.spindle_rpm)
    {
        return Err(PostError::InvalidCuttingConditions);
    }
    if input.moves_work_mm.is_empty() {
        return Err(PostError::EmptyToolpath);
    }
    if input.moves_work_mm.len() > MAX_POST_MOVES {
        return Err(PostError::TooManyMoves);
    }
    for (index, move_) in input.moves_work_mm.iter().enumerate() {
        if !move_
            .from_work_mm
            .iter()
            .chain(move_.to_work_mm.iter())
            .all(|v| v.is_finite())
        {
            return Err(PostError::InvalidCoordinates);
        }
        if move_.from_work_mm == move_.to_work_mm {
            return Err(PostError::ZeroLengthMotion);
        }
        if index > 0 && input.moves_work_mm[index - 1].to_work_mm != move_.from_work_mm {
            return Err(PostError::DiscontinuousPath);
        }
    }

    let mut output = NcWriter::new(profile);
    output.custom(&profile.start_commands)?;
    output.line(profile.units.code())?;
    output.line("G90")?;
    output.line("G94")?;
    output.line("G17")?;
    let spindle = format_number(input.conditions.spindle_rpm, profile.spindle_decimals)?;
    if spindle.parse::<f64>().ok().is_none_or(|value| value <= 0.0) {
        return Err(PostError::InvalidCuttingConditions);
    }
    output.line(&format!("S{spindle}"))?;
    if profile.auto_start_spindle {
        output.line(profile.spindle_direction.code())?;
    }
    let feed = format_number(
        input.conditions.feed_mm_min / profile.units.mm_per_unit(),
        profile.feed_decimals,
    )?;
    if feed.parse::<f64>().ok().is_none_or(|value| value <= 0.0) {
        return Err(PostError::InvalidCuttingConditions);
    }

    let mut last_written_position: Option<[String; 3]> = None;
    let mut feed_written = false;
    for move_ in input.moves_work_mm {
        let from = formatted_position(move_.from_work_mm, profile.units, profile.xyz_decimals)?;
        let to = formatted_position(move_.to_work_mm, profile.units, profile.xyz_decimals)?;
        if to == from {
            return Err(PostError::RoundedMotionLost);
        }
        let mut line = if move_.kind == PlannedMoveKind::RapidTraverse {
            "G0".to_owned()
        } else {
            "G1".to_owned()
        };
        let first = last_written_position.is_none();
        for (axis, i) in [('X', 0), ('Y', 1), ('Z', 2)] {
            if first
                || !profile.suppress_unchanged_axes
                || last_written_position
                    .as_ref()
                    .is_some_and(|p| p[i] != to[i])
            {
                line.push(' ');
                line.push(axis);
                line.push_str(&to[i]);
            }
        }
        // Si los movimientos se redondean hasta convertirse en un mismo
        // destino, abortar: omitirlos silenciosamente falsearia el programa.
        if !first && last_written_position.as_ref().is_some_and(|p| p == &to) {
            return Err(PostError::RoundedMotionLost);
        }
        if move_.kind != PlannedMoveKind::RapidTraverse && !feed_written {
            line.push_str(" F");
            line.push_str(&feed);
            feed_written = true;
        }
        output.line(&line)?;
        last_written_position = Some(to);
    }

    output.custom(&profile.end_commands)?;
    if profile.auto_stop_spindle {
        output.line("M5")?;
    }
    if profile.auto_program_end {
        output.line("M30")?;
    }
    let first_move = input.moves_work_mm.first().expect("Entrada no vacia");
    let last_move = input.moves_work_mm.last().expect("Entrada no vacia");
    Ok(GeneratedNc {
        text: output.text,
        profile_id: profile.id.clone(),
        units: profile.units.as_nc_units(),
        required_first_move_position_work_mm: first_move.from_work_mm,
        last_cam_position_work_mm: last_move.to_work_mm,
        cam_move_count: input.moves_work_mm.len(),
        requires_nc_interpretation: true,
        has_custom_blocks: !profile.start_commands.is_empty() || !profile.end_commands.is_empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moves() -> Vec<PostMove> {
        vec![
            PostMove {
                kind: PlannedMoveKind::RapidTraverse,
                from_work_mm: [0.0, 0.0, 25.4],
                to_work_mm: [25.4, 0.0, 25.4],
                pass_index: 0,
            },
            PostMove {
                kind: PlannedMoveKind::Approach,
                from_work_mm: [25.4, 0.0, 25.4],
                to_work_mm: [25.4, 0.0, 12.7],
                pass_index: 0,
            },
            PostMove {
                kind: PlannedMoveKind::Cutting,
                from_work_mm: [25.4, 0.0, 12.7],
                to_work_mm: [50.8, 0.0, 12.7],
                pass_index: 0,
            },
            PostMove {
                kind: PlannedMoveKind::Retract,
                from_work_mm: [50.8, 0.0, 12.7],
                to_work_mm: [50.8, 0.0, 25.4],
                pass_index: 0,
            },
        ]
    }

    fn conditions() -> ConfiguredCuttingConditions {
        ConfiguredCuttingConditions {
            feed_mm_min: 254.0,
            spindle_rpm: 14000.0,
        }
    }

    #[test]
    fn grbl_defaults_generate_continuous_g0_g1_f_s_and_end() {
        let path = moves();
        let output = generate_program(
            &PostProcessorProfile::grbl_3axis_mm(),
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert_eq!(output.units, NcUnits::Millimeters);
        assert_eq!(
            output.required_first_move_position_work_mm,
            [0.0, 0.0, 25.4]
        );
        assert_eq!(output.cam_move_count, 4);
        assert_eq!(
            output.text,
            concat!(
                "G21\nG90\nG94\nG17\nS14000\nM3\n",
                "G0 X25.400 Y0.000 Z25.400\n",
                "G1 X25.400 Y0.000 Z12.700 F254.000\n",
                "G1 X50.800 Y0.000 Z12.700\n",
                "G1 X50.800 Y0.000 Z25.400\nM5\nM30\n",
            )
        );
        assert!(output.requires_nc_interpretation);
        assert!(!output.has_custom_blocks);
    }

    #[test]
    fn inch_code_converts_geometry_and_feed_not_spindle() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.units = PostUnits::Inches;
        let output = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert_eq!(output.units, NcUnits::Inches);
        assert!(output.text.starts_with("G20\nG90\n"));
        assert!(output.text.contains("G0 X1.000 Y0.000 Z1.000"));
        assert!(output.text.contains("G1 X1.000 Y0.000 Z0.500 F10.000"));
        assert!(output.text.contains("S14000\nM3\n"));
    }

    #[test]
    fn start_and_end_preserve_user_line_order_and_modal_reset() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.start_commands = "(Usuario)\r\nG91 G1 Z1 F10\r\n".into();
        profile.end_commands = "(Terminado)\nM9\n".into();
        let output = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert!(
            output
                .text
                .starts_with("(Usuario)\nG91 G1 Z1 F10\nG21\nG90\n")
        );
        assert!(output.text.ends_with("(Terminado)\nM9\nM5\nM30\n"));
        assert!(output.has_custom_blocks);
        assert!(output.requires_nc_interpretation);
    }

    #[test]
    fn optional_line_numbers_and_crlf_are_applied_to_all_lines() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.line_numbers = Some(PostLineNumbers { start: 5, step: 5 });
        profile.line_ending = PostLineEnding::CrLf;
        profile.start_commands = "(Cabecera)".into();
        let output = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert!(
            output
                .text
                .starts_with("N5 (Cabecera)\r\nN10 G21\r\nN15 G90\r\n")
        );
        assert!(!output.text.contains("\nG1"));
        assert!(output.text.contains("\r\n"));
    }

    #[test]
    fn modal_coordinates_can_be_suppressed_but_first_xyz_are_explicit() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.suppress_unchanged_axes = true;
        let output = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert!(output.text.contains("G0 X25.400 Y0.000 Z25.400\n"));
        assert!(output.text.contains("G1 Z12.700 F254.000\n"));
        assert!(output.text.contains("G1 X50.800\n"));
    }

    #[test]
    fn discontinuity_and_zero_length_are_rejected() {
        let mut path = moves();
        path[1].from_work_mm[0] = 42.0;
        assert_eq!(
            generate_program(
                &PostProcessorProfile::grbl_3axis_mm(),
                PostInput {
                    moves_work_mm: &path,
                    conditions: conditions()
                }
            )
            .unwrap_err(),
            PostError::DiscontinuousPath
        );
        let mut path = moves();
        path[0].to_work_mm = path[0].from_work_mm;
        assert_eq!(
            generate_program(
                &PostProcessorProfile::grbl_3axis_mm(),
                PostInput {
                    moves_work_mm: &path,
                    conditions: conditions()
                }
            )
            .unwrap_err(),
            PostError::ZeroLengthMotion
        );
    }

    #[test]
    fn precision_must_not_disappear_real_movements() {
        let path = [PostMove {
            kind: PlannedMoveKind::Cutting,
            from_work_mm: [0.0, 0.0, 0.0],
            to_work_mm: [0.0001, 0.0, 0.0],
            pass_index: 0,
        }];
        assert_eq!(
            generate_program(
                &PostProcessorProfile::grbl_3axis_mm(),
                PostInput {
                    moves_work_mm: &path,
                    conditions: conditions()
                }
            )
            .unwrap_err(),
            PostError::RoundedMotionLost
        );
    }

    #[test]
    fn profile_is_versioned_and_serializes_without_hidden_defaults() {
        let profile = PostProcessorProfile::grbl_3axis_mm();
        let text = serde_json::to_string(&profile).unwrap();
        let decoded: PostProcessorProfile = serde_json::from_str(&text).unwrap();
        assert_eq!(profile, decoded);
        let mut old = profile.clone();
        old.version = 42;
        assert_eq!(old.validate(), Err(PostError::UnsupportedProfileVersion));
        let extra = text.replacen("\"version\":1", "\"version\":1,\"extra\":true", 1);
        assert!(serde_json::from_str::<PostProcessorProfile>(&extra).is_err());
    }

    #[test]
    fn arbitrary_user_commands_are_kept_but_not_claimed_safe() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.start_commands = "G54\nG92 X2\nM8".into();
        profile.end_commands = "G91 G1 Z1 F10".into();
        profile.auto_program_end = false;
        let output = generate_program(
            &profile,
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert!(output.text.starts_with("G54\nG92 X2\nM8\nG21\n"));
        assert!(output.text.contains("G91 G1 Z1 F10\nM5\n"));
        assert!(!output.text.ends_with("M30\n"));
        assert!(output.requires_nc_interpretation);
    }

    #[test]
    fn invalid_profile_and_controls_are_rejected_before_writing() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.extension = "../exe".into();
        assert_eq!(profile.validate(), Err(PostError::InvalidProfile));
        profile = PostProcessorProfile::grbl_3axis_mm();
        profile.start_commands = "G90\0M30".into();
        assert_eq!(profile.validate(), Err(PostError::InvalidCustomCommands));
        profile = PostProcessorProfile::grbl_3axis_mm();
        profile.xyz_decimals = 99;
        assert_eq!(profile.validate(), Err(PostError::InvalidPrecision));
        profile = PostProcessorProfile::grbl_3axis_mm();
        profile.line_numbers = Some(PostLineNumbers { start: 1, step: 0 });
        assert_eq!(profile.validate(), Err(PostError::InvalidLineNumbers));
        profile = PostProcessorProfile::grbl_3axis_mm();
        let mut cond = conditions();
        cond.feed_mm_min = f64::NAN;
        assert_eq!(
            generate_program(
                &profile,
                PostInput {
                    moves_work_mm: &path,
                    conditions: cond
                }
            )
            .unwrap_err(),
            PostError::InvalidCuttingConditions
        );
    }

    #[test]
    fn tiny_feeds_that_round_to_zero_must_be_rejected() {
        let path = moves();
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.feed_decimals = 0;
        let cond = ConfiguredCuttingConditions {
            feed_mm_min: 0.001,
            spindle_rpm: 14000.0,
        };
        assert_eq!(
            generate_program(
                &profile,
                PostInput {
                    moves_work_mm: &path,
                    conditions: cond
                }
            )
            .unwrap_err(),
            PostError::InvalidCuttingConditions
        );
    }

    #[test]
    fn unsupported_tool_changes_are_not_invented() {
        let path = moves();
        let output = generate_program(
            &PostProcessorProfile::grbl_3axis_mm(),
            PostInput {
                moves_work_mm: &path,
                conditions: conditions(),
            },
        )
        .unwrap();
        assert!(!output.text.contains("M6"));
        assert!(!output.text.contains("G53"));
        assert!(!output.text.contains("G28"));
    }
}

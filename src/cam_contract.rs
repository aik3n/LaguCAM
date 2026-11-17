//! F1.1: contrato de salida CAM independiente de interfaz, OpenGL y G-code.
//!
//! Las posiciones de PlannedToolpath permanecen en coordenadas del STL/modelo
//! y en milimetros. La conversion al origen de trabajo es otra etapa.
//! F y S describen la operacion, no la herramienta ni la maquina.
//! Un contrato valido estructuralmente NO certifica una trayectoria para CNC.

use crate::{toolpath_links::PlannedToolpath, toolpath_validation::ValidationReport};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CamUnits {
    #[default]
    Millimeters,
}

impl CamUnits {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
        }
    }
}

/// Valores elegidos por una estrategia. None significa SIN configurar;
/// nunca sustituir None por un avance o RPM supuestos.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CuttingConditions {
    pub feed_mm_min: Option<f64>,
    pub spindle_rpm: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfiguredCuttingConditions {
    pub feed_mm_min: f64,
    pub spindle_rpm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CuttingConditionsError {
    InvalidFeed,
    InvalidSpindle,
    MissingFeed,
    MissingSpindle,
}

impl fmt::Display for CuttingConditionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFeed => f.write_str("Avance F invalido: debe ser finito y > 0 mm/min"),
            Self::InvalidSpindle => f.write_str("Husillo S invalido: debe ser finito y > 0 rpm"),
            Self::MissingFeed => f.write_str("F de corte sin configurar en la estrategia"),
            Self::MissingSpindle => f.write_str("S de husillo sin configurar en la estrategia"),
        }
    }
}

impl std::error::Error for CuttingConditionsError {}

impl CuttingConditions {
    /// Permite una previsualizacion CAM sin F/S. Si se configuran, han de ser
    /// estrictamente positivos, finitos y estar dentro del limite numerico.
    pub fn validate_optional(self) -> Result<(), CuttingConditionsError> {
        if let Some(feed) = self.feed_mm_min
            && (!feed.is_finite() || feed <= 0.0 || feed > 1_000_000.0)
        {
            return Err(CuttingConditionsError::InvalidFeed);
        }
        if let Some(spindle) = self.spindle_rpm
            && (!spindle.is_finite() || spindle <= 0.0 || spindle > 1_000_000.0)
        {
            return Err(CuttingConditionsError::InvalidSpindle);
        }
        Ok(())
    }

    /// Solo exige que F/S esten presentes y sean validos. NO equivale a una
    /// autorizacion de postprocesado, colisiones ni ejecucion en maquina.
    pub fn require_configured(self) -> Result<ConfiguredCuttingConditions, CuttingConditionsError> {
        self.validate_optional()?;
        Ok(ConfiguredCuttingConditions {
            feed_mm_min: self
                .feed_mm_min
                .ok_or(CuttingConditionsError::MissingFeed)?,
            spindle_rpm: self
                .spindle_rpm
                .ok_or(CuttingConditionsError::MissingSpindle)?,
        })
    }
}

/// Vista inmutable de la salida de CUALQUIER estrategia CAM. No duplica
/// movimientos, no depende de formatos CNC y no transforma el cero de trabajo.
/// El postprocesador futuro tendra que convertir unidades/origen explicitamente.
#[derive(Clone, Copy, Debug)]
pub struct CamOperationView<'a> {
    pub units: CamUnits,
    pub movements: &'a PlannedToolpath,
    pub conditions: CuttingConditions,
    pub structural_validation: &'a ValidationReport,
}

impl<'a> CamOperationView<'a> {
    pub fn new(
        movements: &'a PlannedToolpath,
        structural_validation: &'a ValidationReport,
        conditions: CuttingConditions,
    ) -> Result<Self, CuttingConditionsError> {
        conditions.validate_optional()?;
        Ok(Self {
            units: CamUnits::Millimeters,
            movements,
            conditions,
            structural_validation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mm_is_currently_exposed_and_unset_conditions_are_not_invented() {
        assert_eq!(CamUnits::Millimeters.symbol(), "mm");
        let unset = CuttingConditions::default();
        assert_eq!(unset.validate_optional(), Ok(()));
        assert_eq!(
            unset.require_configured(),
            Err(CuttingConditionsError::MissingFeed)
        );
    }

    #[test]
    fn incomplete_conditions_never_masquerade_as_configured() {
        let only_feed = CuttingConditions {
            feed_mm_min: Some(750.0),
            ..Default::default()
        };
        assert_eq!(
            only_feed.require_configured(),
            Err(CuttingConditionsError::MissingSpindle)
        );
    }

    #[test]
    fn refuses_invalid_finite_infinite_nan_and_out_of_range_values() {
        for invalid in [
            0.0,
            -10.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1_000_000.1,
        ] {
            assert_eq!(
                CuttingConditions {
                    feed_mm_min: Some(invalid),
                    spindle_rpm: None
                }
                .validate_optional(),
                Err(CuttingConditionsError::InvalidFeed)
            );
            assert_eq!(
                CuttingConditions {
                    feed_mm_min: None,
                    spindle_rpm: Some(invalid)
                }
                .validate_optional(),
                Err(CuttingConditionsError::InvalidSpindle)
            );
        }
    }

    #[test]
    fn configured_values_are_preserved_without_rounding() {
        let conditions = CuttingConditions {
            feed_mm_min: Some(1234.56789),
            spindle_rpm: Some(18500.25),
        };
        let configured = conditions.require_configured().unwrap();
        assert_eq!(configured.feed_mm_min.to_bits(), 1234.56789_f64.to_bits());
        assert_eq!(configured.spindle_rpm.to_bits(), 18500.25_f64.to_bits());
    }

    #[test]
    fn operation_borrows_same_path_and_report_without_mutating_them() {
        let path = PlannedToolpath::default();
        let report = ValidationReport::default();
        let view = CamOperationView::new(&path, &report, CuttingConditions::default()).unwrap();
        assert_eq!(view.units, CamUnits::Millimeters);
        assert!(std::ptr::eq(view.movements, &path));
        assert!(std::ptr::eq(view.structural_validation, &report));
    }
}

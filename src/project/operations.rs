//! Operaciones CAM: definiciones persistentes, independientes del visor y de OpenGL.
use crate::tools::model::ToolId;
use cam_libre_visor::cam_contract::CuttingConditions;
use cam_libre_visor::toolpath::RasterPattern;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestAreaXY {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl TestAreaXY {
    pub fn validate(self) -> Result<(), String> {
        if ![self.x_min, self.x_max, self.y_min, self.y_max]
            .iter()
            .all(|v| v.is_finite())
            || self.x_min >= self.x_max
            || self.y_min >= self.y_max
        {
            return Err("Zona XY invalida: minimo inferior al maximo y valores finitos".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelTestParameters {
    pub diameter: f64,
    pub stepover: f64,
    pub longitudinal: f64,
    pub along_x: bool,
    /// Sobreespesor euclideo respecto al STL nominal. No es acumulativo.
    /// La estrategia TEST presupone que las coordenadas del STL son milimetros.
    #[serde(default, skip_serializing_if = "allowance_is_zero")]
    pub allowance_mm: f64,
    /// Avance F propio de la estrategia. None = pendiente; nunca asumir uno.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutting_feed_mm_min: Option<f64>,
    /// Velocidad de husillo S propia de la estrategia. None = pendiente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_rpm: Option<f64>,
    /// Orden de corte de las pasadas: el patrón predeterminado conserva G25.1.
    #[serde(default, skip_serializing_if = "RasterPattern::is_unidirectional")]
    pub pass_pattern: RasterPattern,
    /// Distancia positiva sobre la referencia Z superior del STL y bruto.
    #[serde(
        default = "default_clearance_mm",
        skip_serializing_if = "clearance_is_default"
    )]
    pub clearance_mm: f64,
    /// Distancia positiva sobre la misma referencia para la aproximacion.
    #[serde(
        default = "default_approach_mm",
        skip_serializing_if = "approach_is_default"
    )]
    pub approach_mm: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_xy: Option<TestAreaXY>,
}

fn allowance_is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn default_clearance_mm() -> f64 {
    10.0
}
fn default_approach_mm() -> f64 {
    2.0
}
fn clearance_is_default(value: &f64) -> bool {
    *value == default_clearance_mm()
}
fn approach_is_default(value: &f64) -> bool {
    *value == default_approach_mm()
}

impl Default for ParallelTestParameters {
    fn default() -> Self {
        Self {
            diameter: 4.0,
            stepover: 10.0,
            longitudinal: 2.0,
            along_x: true,
            allowance_mm: 0.0,
            cutting_feed_mm_min: None,
            spindle_rpm: None,
            pass_pattern: RasterPattern::Unidirectional,
            clearance_mm: default_clearance_mm(),
            approach_mm: default_approach_mm(),
            area_xy: None,
        }
    }
}

impl ParallelTestParameters {
    pub fn validate(self) -> Result<(), String> {
        for (label, value) in [
            ("Diametro", self.diameter),
            ("Paso lateral", self.stepover),
            ("Resolucion longitudinal", self.longitudinal),
        ] {
            if !value.is_finite() || value <= 0.0 || value > 1_000_000.0 {
                return Err(format!("{label} de la estrategia paralelo TEST invalido"));
            }
        }
        if !self.diameter.powi(2).is_finite() {
            return Err("Diametro fuera del rango del motor".into());
        }
        if !self.allowance_mm.is_finite()
            || self.allowance_mm < 0.0
            || self.allowance_mm > 1_000_000.0
        {
            return Err("Sobreespesor de la estrategia paralelo TEST invalido".into());
        }
        if !self.approach_mm.is_finite()
            || !self.clearance_mm.is_finite()
            || self.approach_mm <= 0.0
            || self.clearance_mm <= self.approach_mm
            || self.clearance_mm > 1_000_000.0
        {
            return Err("Alturas de movimiento invalidas: despeje > aproximacion > 0".into());
        }
        if let Some(area) = self.area_xy {
            area.validate()?;
        }
        self.cutting_conditions()
            .validate_optional()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn cutting_conditions(self) -> CuttingConditions {
        CuttingConditions {
            feed_mm_min: self.cutting_feed_mm_min,
            spindle_rpm: self.spindle_rpm,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OperationKind {
    Placeholder,
    ParallelTest(ParallelTestParameters),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    NotCalculated,
    Ready,
    Error,
}

#[derive(Clone, Debug)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub tool_id: Option<ToolId>,
    pub toolpath_id: Option<u64>,
}

#[derive(Default)]
pub struct Operations {
    pub items: Vec<Operation>,
}

impl Operations {
    /// El usuario elige la estrategia despues, desde el editor de operacion.
    pub fn add_new(&mut self) -> Result<usize, String> {
        if self.items.len() >= 32 {
            return Err("Limite temporal de 32 operaciones alcanzado".into());
        }
        let mut serial = 1usize;
        while self
            .items
            .iter()
            .any(|op| op.name == format!("Nueva operación {serial:02}"))
        {
            serial += 1;
        }
        let index = self.items.len();
        self.items.push(Operation {
            name: format!("Nueva operación {serial:02}"),
            kind: OperationKind::Placeholder,
            status: OperationStatus::NotCalculated,
            tool_id: None,
            toolpath_id: None,
        });
        Ok(index)
    }

    #[cfg(test)]
    pub fn add_parallel_test(&mut self) -> Result<usize, String> {
        // Cuota temporal: la previsualizacion almacena hasta 12.000 puntos/operacion.
        const MAX_TEST_OPERATIONS: usize = 32;
        if self
            .items
            .iter()
            .filter(|op| matches!(&op.kind, OperationKind::ParallelTest(_)))
            .count()
            >= MAX_TEST_OPERATIONS
        {
            return Err("Limite temporal de 32 operaciones TEST alcanzado".into());
        }
        let serial = 1 + self
            .items
            .iter()
            .filter(|op| matches!(&op.kind, OperationKind::ParallelTest(_)))
            .count();
        let index = self.items.len();
        self.items.push(Operation {
            name: format!("Acabado paralelo TEST {serial:02}"),
            kind: OperationKind::ParallelTest(ParallelTestParameters::default()),
            status: OperationStatus::NotCalculated,
            tool_id: None,
            toolpath_id: None,
        });
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_parameters_default_and_invalid_values() {
        assert!(ParallelTestParameters::default().validate().is_ok());
        let mut p = ParallelTestParameters {
            stepover: f64::NAN,
            ..Default::default()
        };
        assert!(p.validate().is_err());
        p.stepover = 2.0;
        p.diameter = 0.0;
        assert!(p.validate().is_err());
    }
    #[test]
    fn creating_operations_gives_independent_parameters() {
        let mut operations = Operations::default();
        let a = operations.add_parallel_test().unwrap();
        let b = operations.add_parallel_test().unwrap();
        if let OperationKind::ParallelTest(ref mut p) = operations.items[a].kind {
            p.stepover = 1.0;
        }
        if let OperationKind::ParallelTest(p) = &operations.items[b].kind {
            assert_eq!(p.stepover, 10.0);
        } else {
            panic!("La segunda operacion no es paralelo TEST");
        }
    }
}

#[cfg(test)]
mod test_area_xy_compatibility {
    use super::*;

    #[test]
    fn area_optional_and_old_parameters_remain_valid() {
        let defaults = ParallelTestParameters::default();
        let serialized = serde_json::to_value(defaults).unwrap();
        assert!(serialized.get("area_xy").is_none());
        let restored: ParallelTestParameters = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.area_xy, None);
    }

    #[test]
    fn rejects_non_finite_or_inverted_area() {
        let mut p = ParallelTestParameters {
            area_xy: Some(TestAreaXY {
                x_min: 1.0,
                x_max: 0.0,
                y_min: 0.0,
                y_max: 1.0,
            }),
            ..Default::default()
        };
        assert!(p.validate().is_err());
        p.area_xy = Some(TestAreaXY {
            x_min: 0.0,
            x_max: 1.0,
            y_min: f64::NAN,
            y_max: 1.0,
        });
        assert!(p.validate().is_err());
    }
}

#[cfg(test)]
mod new_operation_workflow_tests {
    use super::*;

    #[test]
    fn new_operations_start_without_strategy_and_keep_independent_state() {
        let mut operations = Operations::default();
        let a = operations.add_new().unwrap();
        let b = operations.add_new().unwrap();
        assert_eq!(operations.items[a].name, "Nueva operación 01");
        assert_eq!(operations.items[b].name, "Nueva operación 02");
        assert!(matches!(
            operations.items[a].kind,
            OperationKind::Placeholder
        ));
        assert!(matches!(
            operations.items[b].kind,
            OperationKind::Placeholder
        ));
        operations.items[a].kind = OperationKind::ParallelTest(ParallelTestParameters::default());
        assert!(matches!(
            operations.items[b].kind,
            OperationKind::Placeholder
        ));
    }
}

#[cfg(test)]
mod g23_allowance_tests {
    use super::*;

    #[test]
    fn legacy_json_defaults_to_zero_and_zero_keeps_old_shape() {
        let json = r#"{"diameter":4.0,"stepover":10.0,"longitudinal":2.0,"along_x":true}"#;
        let restored: ParallelTestParameters = serde_json::from_str(json).unwrap();
        assert_eq!(restored.allowance_mm, 0.0);
        assert_eq!(restored.validate(), Ok(()));
        let saved = serde_json::to_value(restored).unwrap();
        assert!(saved.get("allowance_mm").is_none());
    }

    #[test]
    fn allowance_is_independent_and_survives_serialization() {
        let mut roughing = ParallelTestParameters::default();
        let finishing = ParallelTestParameters::default();
        roughing.allowance_mm = 3.25;
        let json = serde_json::to_string(&roughing).unwrap();
        assert_eq!(
            serde_json::from_str::<ParallelTestParameters>(&json).unwrap(),
            roughing
        );
        assert_eq!(finishing.allowance_mm, 0.0);
        assert_eq!(roughing.validate(), Ok(()));
    }

    #[test]
    fn invalid_allowances_are_rejected() {
        for bad in [
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1_000_001.0,
        ] {
            let p = ParallelTestParameters {
                allowance_mm: bad,
                ..Default::default()
            };
            assert!(p.validate().is_err(), "Invalid allowance accepted: {bad:?}");
        }
        let p = ParallelTestParameters {
            allowance_mm: 1_000_000.0,
            ..Default::default()
        };
        assert!(p.validate().is_ok());
    }
}

#[cfg(test)]
mod g25_2_pass_pattern_tests {
    use super::*;

    #[test]
    fn old_projects_default_to_unidirectional_without_schema_change() {
        let params = ParallelTestParameters::default();
        let old_json = serde_json::to_value(params).unwrap();
        assert!(old_json.get("pass_pattern").is_none());
        let restored: ParallelTestParameters = serde_json::from_value(old_json).unwrap();
        assert_eq!(restored.pass_pattern, RasterPattern::Unidirectional);
    }

    #[test]
    fn zigzag_serializes_roundtrips_and_changes_strategy_identity() {
        let baseline = ParallelTestParameters::default();
        let mut zigzag = baseline;
        zigzag.pass_pattern = RasterPattern::Zigzag;
        assert_ne!(zigzag, baseline);
        assert_ne!(
            OperationKind::ParallelTest(zigzag),
            OperationKind::ParallelTest(baseline)
        );
        assert_eq!(zigzag.validate(), Ok(()));
        let json = serde_json::to_value(zigzag).unwrap();
        assert_eq!(json["pass_pattern"].as_str(), Some("zigzag"));
        let restored: ParallelTestParameters = serde_json::from_value(json).unwrap();
        assert_eq!(restored.pass_pattern, RasterPattern::Zigzag);
    }

    #[test]
    fn rejects_unknown_pass_pattern_without_silent_fallback() {
        let mut json = serde_json::to_value(ParallelTestParameters::default()).unwrap();
        json["pass_pattern"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<ParallelTestParameters>(json).is_err());
    }
}

#[cfg(test)]
mod f1_1_cutting_conditions_tests {
    use super::*;

    #[test]
    fn legacy_json_does_not_gain_invented_feeds_or_spindle_values() {
        let json = r#"{"diameter":4.0,"stepover":10.0,"longitudinal":2.0,"along_x":true}"#;
        let restored: ParallelTestParameters = serde_json::from_str(json).unwrap();
        assert_eq!(restored.cutting_feed_mm_min, None);
        assert_eq!(restored.spindle_rpm, None);
        assert_eq!(restored.validate(), Ok(()));
        let value = serde_json::to_value(restored).unwrap();
        assert!(value.get("cutting_feed_mm_min").is_none());
        assert!(value.get("spindle_rpm").is_none());
    }

    #[test]
    fn configured_feed_and_rpm_roundtrip_independently() {
        let mut first = ParallelTestParameters::default();
        let second = ParallelTestParameters::default();
        first.cutting_feed_mm_min = Some(1200.25);
        first.spindle_rpm = Some(14000.5);
        let json = serde_json::to_string(&first).unwrap();
        assert_eq!(
            serde_json::from_str::<ParallelTestParameters>(&json).unwrap(),
            first
        );
        assert_eq!(second.cutting_feed_mm_min, None);
        assert_eq!(second.spindle_rpm, None);
        assert_ne!(
            OperationKind::ParallelTest(first),
            OperationKind::ParallelTest(second)
        );
        assert_eq!(first.validate(), Ok(()));
    }

    #[test]
    fn configured_invalid_feed_and_spindle_are_rejected() {
        for value in [0.0, -10.0, f64::NAN, f64::INFINITY, 1_000_001.0] {
            let mut params = ParallelTestParameters {
                cutting_feed_mm_min: Some(value),
                ..Default::default()
            };
            assert!(params.validate().is_err());
            params.cutting_feed_mm_min = None;
            params.spindle_rpm = Some(value);
            assert!(params.validate().is_err());
        }
    }
}

#[cfg(test)]
mod g25_3_heights_tests {
    use super::*;

    #[test]
    fn legacy_json_uses_defaults_without_changing_old_json_shape() {
        let defaults = ParallelTestParameters::default();
        assert_eq!((defaults.clearance_mm, defaults.approach_mm), (10.0, 2.0));
        let json = serde_json::to_value(defaults).unwrap();
        assert!(json.get("clearance_mm").is_none());
        assert!(json.get("approach_mm").is_none());
        let restored: ParallelTestParameters = serde_json::from_value(json).unwrap();
        assert_eq!(restored, defaults);
    }

    #[test]
    fn serializes_independent_heights_and_rejects_invalid_levels() {
        let params = ParallelTestParameters {
            clearance_mm: 17.0,
            approach_mm: 3.5,
            ..Default::default()
        };
        let json = serde_json::to_value(params).unwrap();
        assert_eq!(json["clearance_mm"].as_f64(), Some(17.0));
        assert_eq!(json["approach_mm"].as_f64(), Some(3.5));
        let restored: ParallelTestParameters = serde_json::from_value(json).unwrap();
        assert_eq!(restored, params);
        for (approach, clearance) in [
            (0.0, 10.0),
            (-1.0, 10.0),
            (5.0, 5.0),
            (10.0, 3.0),
            (f64::NAN, 10.0),
            (2.0, f64::INFINITY),
            (2.0, 1_000_001.0),
        ] {
            let candidate = ParallelTestParameters {
                approach_mm: approach,
                clearance_mm: clearance,
                ..Default::default()
            };
            assert!(candidate.validate().is_err());
        }
    }
}

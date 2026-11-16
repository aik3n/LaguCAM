//! Operaciones CAM: definiciones persistentes, independientes del visor y de OpenGL.
use crate::tools::model::ToolId;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_xy: Option<TestAreaXY>,
}

fn allowance_is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl Default for ParallelTestParameters {
    fn default() -> Self {
        Self {
            diameter: 4.0,
            stepover: 10.0,
            longitudinal: 2.0,
            along_x: true,
            allowance_mm: 0.0,
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
        if let Some(area) = self.area_xy {
            area.validate()?;
        }
        Ok(())
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

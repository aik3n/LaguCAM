//! F1.2: proyeccion de movimientos CAM desde el STL/modelo al cero de trabajo.
//!
//! No modifica ni redondea la trayectoria de origen. No genera G-code ni
//! determina posiciones iniciales de maquina. Solamente se admite un sistema
//! de tres ejes cuyo Z de trabajo sea paralelo al Z del modelo (rotacion Rz).
//! Inclinar Rx/Ry exigiria mecanizado indexado u otra configuracion de maquina.

use crate::{
    project::{origin::WorkOrigin, stock::Stock},
    stl::Bounds,
};
use cam_libre_visor::{
    cam_contract::{CamOperationView, CamUnits},
    toolpath_links::{PlannedMoveKind, PlannedToolpath},
    toolpath_validation::MAX_VALIDATION_MOVES,
};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkCoordinateError {
    InvalidStock,
    InvalidBounds,
    InvalidOrigin,
    UnsupportedTilt,
    InvalidPoint,
    ArithmeticOverflow,
    TooManyMoves,
}

impl fmt::Display for WorkCoordinateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStock => f.write_str("Bruto no valido para convertir a coordenadas de trabajo"),
            Self::InvalidBounds => f.write_str("Limites STL no validos para convertir coordenadas"),
            Self::InvalidOrigin => f.write_str("Cero de trabajo no valido"),
            Self::UnsupportedTilt => f.write_str(
                "F1.2: Rx/Ry del cero de trabajo no soportados en CAM de tres ejes; solo giro alrededor de Z",
            ),
            Self::InvalidPoint => f.write_str("Punto CAM con coordenadas no finitas"),
            Self::ArithmeticOverflow => f.write_str("Conversion del punto CAM fuera del rango numerico"),
            Self::TooManyMoves => f.write_str("Demasiados movimientos CAM para convertir"),
        }
    }
}

impl std::error::Error for WorkCoordinateError {}

/// Transformacion inmutable (mm): modelo -> coordenadas de trabajo.
/// Las columnas de `basis` representan los ejes de trabajo en el modelo.
/// No depende del render ni del postprocesador.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkFrame {
    origin_in_model_mm: [f64; 3],
    basis: [[f64; 3]; 3],
}

/// El nombre de los campos evita confundir XYZ de modelo y XYZ de trabajo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkMove {
    pub kind: PlannedMoveKind,
    pub from_work_mm: [f64; 3],
    pub to_work_mm: [f64; 3],
    pub pass_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkToolpath {
    pub units: CamUnits,
    pub moves: Vec<WorkMove>,
}

impl WorkFrame {
    pub fn new(
        origin: &WorkOrigin,
        stock: &Stock,
        model_bounds: &Bounds,
    ) -> Result<Self, WorkCoordinateError> {
        stock
            .validate()
            .map_err(|_| WorkCoordinateError::InvalidStock)?;
        if !model_bounds
            .min
            .iter()
            .zip(model_bounds.max.iter())
            .all(|(a, b)| a.is_finite() && b.is_finite() && b >= a && (b - a).is_finite())
        {
            return Err(WorkCoordinateError::InvalidBounds);
        }
        // No basta con comprobar las trayectorias: una inclinacion del sistema
        // de trabajo altera la orientacion de la fresa respecto del modelo.
        // Rz si es admisible, pues conserva el eje vertical de tres ejes.
        if !origin.rotation_degrees.iter().all(|x| x.is_finite()) {
            return Err(WorkCoordinateError::InvalidOrigin);
        }
        if origin.rotation_degrees[..2]
            .iter()
            .any(|angle| angle.rem_euclid(360.0) != 0.0)
        {
            return Err(WorkCoordinateError::UnsupportedTilt);
        }
        let origin_in_model_mm = origin
            .model_position(stock, model_bounds)
            .map_err(|_| WorkCoordinateError::InvalidOrigin)?;
        let basis = origin
            .rotation_basis()
            .map_err(|_| WorkCoordinateError::InvalidOrigin)?;
        Ok(Self {
            origin_in_model_mm,
            basis,
        })
    }

    pub fn model_to_work(&self, model_point_mm: [f64; 3]) -> Result<[f64; 3], WorkCoordinateError> {
        if !model_point_mm.iter().all(|x| x.is_finite()) {
            return Err(WorkCoordinateError::InvalidPoint);
        }
        let delta: [f64; 3] =
            std::array::from_fn(|i| model_point_mm[i] - self.origin_in_model_mm[i]);
        if !delta.iter().all(|x| x.is_finite()) {
            return Err(WorkCoordinateError::ArithmeticOverflow);
        }
        let result = self
            .basis
            .map(|axis| axis[0] * delta[0] + axis[1] * delta[1] + axis[2] * delta[2]);
        if !result.iter().all(|x| x.is_finite()) {
            return Err(WorkCoordinateError::ArithmeticOverflow);
        }
        Ok(result)
    }

    /// Devuelve una COPIA transformada y tipada; `path` nunca se modifica.
    pub fn convert(&self, path: &PlannedToolpath) -> Result<WorkToolpath, WorkCoordinateError> {
        if path.moves.len() > MAX_VALIDATION_MOVES {
            return Err(WorkCoordinateError::TooManyMoves);
        }
        let mut moves = Vec::with_capacity(path.moves.len());
        for movement in &path.moves {
            moves.push(WorkMove {
                kind: movement.kind,
                from_work_mm: self.model_to_work(movement.from)?,
                to_work_mm: self.model_to_work(movement.to)?,
                pass_index: movement.pass_index,
            });
        }
        Ok(WorkToolpath {
            units: CamUnits::Millimeters,
            moves,
        })
    }

    /// Entrada directa desde F1.1: conserva el contrato de estrategia.
    /// F/S y el informe siguen disponibles en `operation` sin duplicarlos.
    pub fn convert_operation(
        &self,
        operation: &CamOperationView<'_>,
    ) -> Result<WorkToolpath, WorkCoordinateError> {
        self.convert(operation.movements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::origin::{OriginPoint, OriginReference};
    use cam_libre_visor::{
        cam_contract::CuttingConditions,
        toolpath_links::{PlannedMove, PlannedMoveKind},
        toolpath_validation::ValidationReport,
    };

    fn bounds() -> Bounds {
        Bounds {
            min: [10.0, 20.0, 30.0],
            max: [110.0, 120.0, 50.0],
        }
    }
    fn stock() -> Stock {
        Stock {
            min: [0.0, 10.0, 20.0],
            size: [120.0, 120.0, 40.0],
            visible: true,
        }
    }
    fn near(a: [f64; 3], b: [f64; 3]) {
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1.0e-9, "eje {i}: {a:?} != {b:?}");
        }
    }
    fn planned() -> PlannedToolpath {
        PlannedToolpath {
            moves: vec![
                PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from: [10.0, 20.0, 70.0],
                    to: [10.0, 20.0, 40.0],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::Cutting,
                    from: [10.0, 20.0, 40.0],
                    to: [20.0, 20.0, 40.0],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::Retract,
                    from: [20.0, 20.0, 40.0],
                    to: [20.0, 20.0, 70.0],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::RapidTraverse,
                    from: [20.0, 20.0, 70.0],
                    to: [20.0, 30.0, 70.0],
                    pass_index: 1,
                },
            ],
            reference_z: 60.0,
            approach_z: 65.0,
            clearance_z: 70.0,
        }
    }

    #[test]
    fn default_origin_is_top_of_stock_and_keeps_mm() {
        let frame = WorkFrame::new(&WorkOrigin::default(), &stock(), &bounds()).unwrap();
        near(frame.model_to_work([0.0, 10.0, 60.0]).unwrap(), [0.0; 3]);
        near(
            frame.model_to_work([10.0, 20.0, 40.0]).unwrap(),
            [10.0, 10.0, -20.0],
        );
    }

    #[test]
    fn model_anchor_and_offset_are_applied_once() {
        let origin = WorkOrigin {
            reference: OriginReference::Model,
            point: OriginPoint::Center,
            offset: [2.0, -3.0, 1.0],
            ..WorkOrigin::default()
        };
        let frame = WorkFrame::new(&origin, &stock(), &bounds()).unwrap();
        // Centro modelo [60,70,40], offset [2,-3,1].
        near(frame.model_to_work([62.0, 67.0, 41.0]).unwrap(), [0.0; 3]);
        near(
            frame.model_to_work([72.0, 87.0, 46.0]).unwrap(),
            [10.0, 20.0, 5.0],
        );
    }

    #[test]
    fn rotation_around_z_is_supported_and_invertible() {
        let origin = WorkOrigin {
            rotation_degrees: [0.0, 0.0, 90.0],
            ..Default::default()
        };
        let frame = WorkFrame::new(&origin, &stock(), &bounds()).unwrap();
        let work = frame.model_to_work([10.0, 10.0, 60.0]).unwrap();
        near(work, [0.0, -10.0, 0.0]);
        near(
            origin.work_to_model(&stock(), &bounds(), work).unwrap(),
            [10.0, 10.0, 60.0],
        );
    }

    #[test]
    fn tilted_work_axes_are_rejected_for_three_axis_cam() {
        for tilt in [[1.0, 0.0, 0.0], [0.0, 45.0, 0.0], [180.0, 0.0, 0.0]] {
            let origin = WorkOrigin {
                rotation_degrees: tilt,
                ..Default::default()
            };
            assert_eq!(
                WorkFrame::new(&origin, &stock(), &bounds()),
                Err(WorkCoordinateError::UnsupportedTilt)
            );
        }
        let origin = WorkOrigin {
            rotation_degrees: [360.0, -360.0, 30.0],
            ..Default::default()
        };
        assert!(WorkFrame::new(&origin, &stock(), &bounds()).is_ok());
    }

    #[test]
    fn movements_keep_kind_pass_order_and_original_xyz() {
        let source = planned();
        let copy = source.clone();
        let frame = WorkFrame::new(&WorkOrigin::default(), &stock(), &bounds()).unwrap();
        let output = frame.convert(&source).unwrap();
        assert_eq!(output.units, CamUnits::Millimeters);
        assert_eq!(output.moves.len(), source.moves.len());
        for (src, dst) in source.moves.iter().zip(&output.moves) {
            assert_eq!((dst.kind, dst.pass_index), (src.kind, src.pass_index));
            near(dst.from_work_mm, frame.model_to_work(src.from).unwrap());
            near(dst.to_work_mm, frame.model_to_work(src.to).unwrap());
        }
        assert_eq!(source, copy);
    }

    #[test]
    fn yaw_preserves_horizontal_rapid_z() {
        let origin = WorkOrigin {
            rotation_degrees: [0.0, 0.0, 45.0],
            ..Default::default()
        };
        let frame = WorkFrame::new(&origin, &stock(), &bounds()).unwrap();
        let path = frame.convert(&planned()).unwrap();
        let rapid = path.moves.last().unwrap();
        assert_eq!(rapid.kind, PlannedMoveKind::RapidTraverse);
        assert!((rapid.from_work_mm[2] - rapid.to_work_mm[2]).abs() < 1.0e-12);
    }

    #[test]
    fn origin_changes_reproject_without_regenerating_or_mutating_path() {
        let source = planned();
        let first = WorkFrame::new(&WorkOrigin::default(), &stock(), &bounds()).unwrap();
        let changed = WorkOrigin {
            offset: [7.0, 0.0, 0.0],
            ..Default::default()
        };
        let second = WorkFrame::new(&changed, &stock(), &bounds()).unwrap();
        let a = first.convert(&source).unwrap();
        let b = second.convert(&source).unwrap();
        assert!((a.moves[0].from_work_mm[0] - b.moves[0].from_work_mm[0] - 7.0).abs() < 1e-9);
        assert_eq!(source.moves[0].from, [10.0, 20.0, 70.0]);
    }

    #[test]
    fn non_finite_and_overflowing_points_are_rejected() {
        let frame = WorkFrame::new(&WorkOrigin::default(), &stock(), &bounds()).unwrap();
        assert_eq!(
            frame.model_to_work([f64::NAN, 0.0, 0.0]),
            Err(WorkCoordinateError::InvalidPoint)
        );
        let mut source = planned();
        source.moves[0].from[0] = f64::INFINITY;
        assert_eq!(
            frame.convert(&source),
            Err(WorkCoordinateError::InvalidPoint)
        );
        let extreme = WorkOrigin {
            offset: [-f64::MAX, 0.0, 0.0],
            ..Default::default()
        };
        let huge = WorkFrame::new(&extreme, &stock(), &bounds()).unwrap();
        assert_eq!(
            huge.model_to_work([f64::MAX, 0.0, 0.0]),
            Err(WorkCoordinateError::ArithmeticOverflow)
        );
    }

    #[test]
    fn invalid_bounds_and_stock_fail_closed() {
        let invalid_bounds = Bounds {
            min: [2.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        assert_eq!(
            WorkFrame::new(&WorkOrigin::default(), &stock(), &invalid_bounds),
            Err(WorkCoordinateError::InvalidBounds)
        );
        let invalid_stock = Stock {
            size: [0.0; 3],
            ..stock()
        };
        assert_eq!(
            WorkFrame::new(&WorkOrigin::default(), &invalid_stock, &bounds()),
            Err(WorkCoordinateError::InvalidStock)
        );
    }

    #[test]
    fn empty_paths_remain_empty_and_f1_1_contract_is_usable() {
        let frame = WorkFrame::new(&WorkOrigin::default(), &stock(), &bounds()).unwrap();
        let empty = PlannedToolpath::default();
        let report = ValidationReport::default();
        let operation =
            CamOperationView::new(&empty, &report, CuttingConditions::default()).unwrap();
        let result = frame.convert_operation(&operation).unwrap();
        assert_eq!(result.units, CamUnits::Millimeters);
        assert!(result.moves.is_empty());
    }
}

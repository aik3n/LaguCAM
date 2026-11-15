//! Contrato G19 del Contact Solver, independiente de la UI y de las estrategias.
//!
//! Todas las magnitudes se expresan en las unidades y coordenadas originales del
//! STL. Esta primera implementacion solo resuelve el apoyo de una fresa esferica
//! cuyo eje esta alineado con +Z del modelo. No transforma el setup CNC.
//!
//! `allowance_model` >= 0 representa separacion euclidea uniforme respecto a
//! la superficie triangulada para la envolvente accesible desde +Z. NO equivale
//! a simular el solido del bruto ni certifica colisiones, penetracion de mango,
//! zonas inaccesibles, STL cerrados o movimientos seguros.

use super::service::{
    BallContactEngine, BallContactQuery, ContactEngineError, ContactEngineLimits,
    ContactEngineOpenReport,
};
use super::stl::G16AccessMode;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Perfil geometrico matematico. No incluye UUID, nombre, avances ni RPM.
///
/// Las variantes distintas de `BallNose` estan reservadas para ampliar el
/// contrato. Por ahora producen `UnsupportedGeometry`, nunca contacto ficticio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CutterGeometry {
    BallNose {
        diameter_model: f64,
    },
    Flat {
        diameter_model: f64,
    },
    BullNose {
        diameter_model: f64,
        corner_radius_model: f64,
    },
    VBit {
        diameter_model: f64,
        included_angle_deg: f64,
    },
    ConicalFlat {
        diameter_model: f64,
        tip_diameter_model: f64,
        included_angle_deg: f64,
    },
    ConicalBall {
        diameter_model: f64,
        tip_sphere_radius_model: f64,
        included_angle_deg: f64,
    },
}

/// Consulta de posicionamiento vertical de una fresa sobre el STL.
///
/// `x_model`, `y_model` fijan el eje de herramienta en el marco del STL;
/// `allowance_model` se mide desde la pieza nominal, no desde otra operacion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactQuery {
    pub x_model: f64,
    pub y_model: f64,
    pub cutter: CutterGeometry,
    pub allowance_model: f64,
}

impl ContactQuery {
    fn checked_radius(self) -> Result<f64, ContactSolverError> {
        if !self.x_model.is_finite()
            || !self.y_model.is_finite()
            || !self.allowance_model.is_finite()
            || self.allowance_model < 0.0
        {
            return Err(ContactSolverError::InvalidQuery(
                "XY y sobrematerial deben ser finitos; sobrematerial >= 0",
            ));
        }
        let valid_positive = |v: f64| v.is_finite() && v > 0.0;
        let valid_angle = |v: f64| v.is_finite() && v > 0.0 && v < 180.0;
        let diameter = match self.cutter {
            CutterGeometry::BallNose { diameter_model }
            | CutterGeometry::Flat { diameter_model } => diameter_model,
            CutterGeometry::BullNose {
                diameter_model,
                corner_radius_model,
            } => {
                if !valid_positive(corner_radius_model)
                    || corner_radius_model > diameter_model * 0.5
                {
                    return Err(ContactSolverError::InvalidQuery(
                        "Radio de esquina invalido",
                    ));
                }
                diameter_model
            }
            CutterGeometry::VBit {
                diameter_model,
                included_angle_deg,
            } => {
                if !valid_angle(included_angle_deg) {
                    return Err(ContactSolverError::InvalidQuery("Angulo conico invalido"));
                }
                diameter_model
            }
            CutterGeometry::ConicalFlat {
                diameter_model,
                tip_diameter_model,
                included_angle_deg,
            } => {
                if !valid_positive(tip_diameter_model)
                    || tip_diameter_model >= diameter_model
                    || !valid_angle(included_angle_deg)
                {
                    return Err(ContactSolverError::InvalidQuery(
                        "Punta conica plana invalida",
                    ));
                }
                diameter_model
            }
            CutterGeometry::ConicalBall {
                diameter_model,
                tip_sphere_radius_model,
                included_angle_deg,
            } => {
                if !valid_positive(tip_sphere_radius_model)
                    || !valid_angle(included_angle_deg)
                    || tip_sphere_radius_model * (included_angle_deg.to_radians() * 0.5).cos()
                        >= diameter_model * 0.5
                {
                    return Err(ContactSolverError::InvalidQuery(
                        "Punta conica esferica invalida",
                    ));
                }
                diameter_model
            }
        };
        if !valid_positive(diameter) || diameter * 0.5 == 0.0 {
            return Err(ContactSolverError::InvalidQuery(
                "Diametro de fresa invalido",
            ));
        }
        if !matches!(self.cutter, CutterGeometry::BallNose { .. }) {
            return Err(ContactSolverError::UnsupportedGeometry);
        }
        let effective_radius = diameter * 0.5 + self.allowance_model;
        let radius_squared = effective_radius * effective_radius;
        if !effective_radius.is_finite() || !radius_squared.is_finite() || radius_squared == 0.0 {
            return Err(ContactSolverError::ArithmeticRange);
        }
        Ok(effective_radius)
    }
}

#[derive(Debug)]
pub enum ContactSolverError {
    InvalidQuery(&'static str),
    UnsupportedGeometry,
    ArithmeticRange,
    Engine(ContactEngineError),
}

impl fmt::Display for ContactSolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "Consulta de contacto invalida: {message}"),
            Self::UnsupportedGeometry => {
                f.write_str("Geometria de fresa aun no soportada por el Contact Solver")
            }
            Self::ArithmeticRange => f.write_str("Sobrematerial o coordenadas fuera de rango"),
            Self::Engine(error) => write!(f, "Motor geometrico: {error}"),
        }
    }
}
impl std::error::Error for ContactSolverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Engine(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ContactEngineError> for ContactSolverError {
    fn from(value: ContactEngineError) -> Self {
        Self::Engine(value)
    }
}

/// Altura admisible de la punta mas baja; `None` NO significa desplazamiento seguro.
/// Los campos restantes son contadores de diagnostico del indice espacial.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSolution {
    pub tip_z_model: Option<f64>,
    pub triangles_considered: usize,
    pub nodes_visited: usize,
    pub physical_block_reads: usize,
    pub mode: G16AccessMode,
}

/// Resuelve posicionamiento de herramienta sobre la envolvente del STL.
/// Se apoya en el servicio esferico y no contiene patrones de estrategias CAM.
pub struct ContactSolver {
    engine: BallContactEngine,
}

impl ContactSolver {
    pub fn open_stl(
        path: &Path,
        limits: ContactEngineLimits,
        cancelled: &AtomicBool,
    ) -> Result<(Self, ContactEngineOpenReport), ContactEngineError> {
        let (engine, report) = BallContactEngine::open_stl(path, limits, cancelled)?;
        Ok((Self { engine }, report))
    }

    pub fn solve(
        &mut self,
        query: ContactQuery,
        cancelled: &AtomicBool,
    ) -> Result<ContactSolution, ContactSolverError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactSolverError::Engine(ContactEngineError::Cancelled));
        }
        let effective_radius = query.checked_radius()?;
        // Envolvente de una esfera real (radio r) a distancia a de una
        // triangulacion: resolver contacto con radio r+a y sumar a la altura
        // de la punta obtenida. Para una rampa inclinada esto NO equivale
        // a sumar a directamente a la Z de contacto nominal.
        let raw = self.engine.query(
            BallContactQuery {
                x_model: query.x_model,
                y_model: query.y_model,
                radius_model: effective_radius,
            },
            cancelled,
        )?;
        let tip_z_model = match raw.tip_z_model {
            Some(z) if query.allowance_model > 0.0 => {
                let raised = z + query.allowance_model;
                if !raised.is_finite() {
                    return Err(ContactSolverError::ArithmeticRange);
                }
                Some(raised)
            }
            other => other, // En a=0 conservar exactamente los bits de G18.
        };
        Ok(ContactSolution {
            tip_z_model,
            triangles_considered: raw.triangles_considered,
            nodes_visited: raw.nodes_visited,
            physical_block_reads: raw.physical_block_reads,
            mode: raw.mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cutter: CutterGeometry, allowance_model: f64) -> ContactQuery {
        ContactQuery {
            x_model: 5.0,
            y_model: 5.0,
            cutter,
            allowance_model,
        }
    }

    #[test]
    fn rejects_invalid_allowance_and_diameter() {
        for allowance in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                sample(
                    CutterGeometry::BallNose {
                        diameter_model: 4.0
                    },
                    allowance
                )
                .checked_radius(),
                Err(ContactSolverError::InvalidQuery(_))
            ));
        }
        assert!(matches!(
            sample(
                CutterGeometry::BallNose {
                    diameter_model: 0.0
                },
                0.0
            )
            .checked_radius(),
            Err(ContactSolverError::InvalidQuery(_))
        ));
        assert!(matches!(
            sample(
                CutterGeometry::BallNose {
                    diameter_model: f64::MAX
                },
                0.0
            )
            .checked_radius(),
            Err(ContactSolverError::ArithmeticRange)
        ));
    }

    #[test]
    fn future_geometries_are_never_accepted_as_ball_nose() {
        for cutter in [
            CutterGeometry::Flat {
                diameter_model: 6.0,
            },
            CutterGeometry::BullNose {
                diameter_model: 8.0,
                corner_radius_model: 1.0,
            },
            CutterGeometry::VBit {
                diameter_model: 5.0,
                included_angle_deg: 60.0,
            },
            CutterGeometry::ConicalFlat {
                diameter_model: 6.0,
                tip_diameter_model: 0.1,
                included_angle_deg: 30.0,
            },
            CutterGeometry::ConicalBall {
                diameter_model: 6.0,
                tip_sphere_radius_model: 0.5,
                included_angle_deg: 30.0,
            },
        ] {
            assert!(matches!(
                sample(cutter, 1.0).checked_radius(),
                Err(ContactSolverError::UnsupportedGeometry)
            ));
        }
    }
}

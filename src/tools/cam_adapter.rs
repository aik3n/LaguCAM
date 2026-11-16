//! Adaptador G20 entre la biblioteca de fresas y el Contact Solver.
//!
//! El motor recibe un perfil matematico, sin conocer UUID, catalogos o UI.
//! La biblioteca define dimensiones en mm; el llamante debe proporcionar
//! explicitamente cuantas unidades del STL corresponden a un milimetro.
//! En el TEST actual se usa escala 1.0 bajo la hipotesis STL en milimetros.

use super::{
    hash::geometry_sha256,
    model::{ToolGeometry, ToolLibrary},
};
use cam_libre_visor::cam_geometry::contact_solver::CutterGeometry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSignature {
    pub tool_id: String,
    pub geometry_hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ResolvedCutter {
    pub signature: ToolSignature,
    pub cutter: CutterGeometry,
}

impl ResolvedCutter {
    pub fn diameter_model(&self) -> f64 {
        match self.cutter {
            CutterGeometry::BallNose { diameter_model }
            | CutterGeometry::Flat { diameter_model }
            | CutterGeometry::BullNose { diameter_model, .. }
            | CutterGeometry::VBit { diameter_model, .. }
            | CutterGeometry::ConicalFlat { diameter_model, .. }
            | CutterGeometry::ConicalBall { diameter_model, .. } => diameter_model,
        }
    }

    /// G21 resuelve fresas esfericas y planas (estas ultimas sin sobrematerial).
    pub fn ensure_supported(&self) -> Result<(), String> {
        if matches!(
            self.cutter,
            CutterGeometry::BallNose { .. }
                | CutterGeometry::Flat { .. }
                | CutterGeometry::BullNose { .. }
        ) {
            Ok(())
        } else {
            Err("Esta geometria de fresa todavia no tiene Contact Solver implementado".into())
        }
    }
}

/// Resuelve UUID y convierte las dimensiones fisicas a unidades del STL.
/// No se permite usar diametros antiguos guardados en la estrategia como fallback.
pub fn resolve_cutter(
    library: &ToolLibrary,
    tool_id: Option<&str>,
    model_units_per_mm: f64,
) -> Result<ResolvedCutter, String> {
    if !model_units_per_mm.is_finite() || model_units_per_mm <= 0.0 {
        return Err("Escala de unidades STL/mm invalida".into());
    }
    let id = tool_id.ok_or("Selecciona una fresa de la biblioteca para generar")?;
    let tool = library
        .get(id)
        .ok_or("La fresa asignada ya no existe en la biblioteca")?;
    tool.validate()?;
    let geometry_hash = geometry_sha256(tool)?;
    let mm = |value: f64| -> Result<f64, String> {
        let model_value = value * model_units_per_mm;
        if !model_value.is_finite() || model_value <= 0.0 {
            Err("Dimensiones de fresa fuera del rango de unidades STL".into())
        } else {
            Ok(model_value)
        }
    };
    let cutter = match &tool.geometry {
        ToolGeometry::Flat {
            cutting_diameter_mm,
        } => CutterGeometry::Flat {
            diameter_model: mm(*cutting_diameter_mm)?,
        },
        ToolGeometry::BallNose {
            cutting_diameter_mm,
        } => CutterGeometry::BallNose {
            diameter_model: mm(*cutting_diameter_mm)?,
        },
        ToolGeometry::BullNose {
            cutting_diameter_mm,
            corner_radius_mm,
        } => CutterGeometry::BullNose {
            diameter_model: mm(*cutting_diameter_mm)?,
            corner_radius_model: mm(*corner_radius_mm)?,
        },
        ToolGeometry::VBit {
            cutting_diameter_mm,
            included_angle_deg,
        } => CutterGeometry::VBit {
            diameter_model: mm(*cutting_diameter_mm)?,
            included_angle_deg: *included_angle_deg,
        },
        ToolGeometry::ConicalFlat {
            cutting_diameter_mm,
            tip_diameter_mm,
            included_angle_deg,
        } => CutterGeometry::ConicalFlat {
            diameter_model: mm(*cutting_diameter_mm)?,
            tip_diameter_model: mm(*tip_diameter_mm)?,
            included_angle_deg: *included_angle_deg,
        },
        ToolGeometry::ConicalBall {
            cutting_diameter_mm,
            tip_sphere_radius_mm,
            included_angle_deg,
        } => CutterGeometry::ConicalBall {
            diameter_model: mm(*cutting_diameter_mm)?,
            tip_sphere_radius_model: mm(*tip_sphere_radius_mm)?,
            included_angle_deg: *included_angle_deg,
        },
    };
    Ok(ResolvedCutter {
        signature: ToolSignature {
            tool_id: id.to_owned(),
            geometry_hash,
        },
        cutter,
    })
}

/// La operacion y la previsualizacion siguen referidas a la misma geometria.
/// Renombrar la fresa o cambiar sus avances no invalida; cambiar sus
/// dimensiones, mango o longitud si lo hace por seguridad futura.
pub fn signature_current(
    library: &ToolLibrary,
    assigned_id: Option<&str>,
    signature: &ToolSignature,
) -> bool {
    if assigned_id != Some(signature.tool_id.as_str()) {
        return false;
    }
    library
        .get(&signature.tool_id)
        .and_then(|tool| geometry_sha256(tool).ok())
        .is_some_and(|hash| hash == signature.geometry_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::model::{Tool, ToolKind};

    fn library_with(geometry: ToolGeometry) -> (ToolLibrary, String) {
        let mut library = ToolLibrary::default();
        let mut tool = Tool::blank(ToolKind::BallNose);
        tool.name = "Fresa de prueba".into();
        tool.geometry = geometry;
        tool.cutting_length_mm = 20.0;
        tool.shank_diameter_mm = 6.0;
        tool.total_length_mm = 40.0;
        let id = library.add(tool).unwrap();
        (library, id)
    }

    #[test]
    fn bull_library_adapter_preserves_corner_and_invalidates_hash() {
        let (mut lib, id) = library_with(ToolGeometry::BullNose {
            cutting_diameter_mm: 12.0,
            corner_radius_mm: 1.0,
        });
        let resolved = resolve_cutter(&lib, Some(&id), 2.0).unwrap();
        assert_eq!(
            resolved.cutter,
            CutterGeometry::BullNose {
                diameter_model: 24.0,
                corner_radius_model: 2.0,
            }
        );
        resolved.ensure_supported().unwrap();
        let mut updated = lib.get(&id).unwrap().clone();
        updated.geometry = ToolGeometry::BullNose {
            cutting_diameter_mm: 12.0,
            corner_radius_mm: 2.0,
        };
        lib.replace(updated).unwrap();
        assert!(!signature_current(&lib, Some(&id), &resolved.signature));
    }

    #[test]
    fn ball_uses_library_diameter_and_explicit_stl_units() {
        let (lib, id) = library_with(ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        });
        let tool = resolve_cutter(&lib, Some(&id), 2.0).unwrap();
        assert_eq!(
            tool.cutter,
            CutterGeometry::BallNose {
                diameter_model: 8.0
            }
        );
        assert_eq!(tool.diameter_model(), 8.0);
        tool.ensure_supported().unwrap();
        assert!(signature_current(&lib, Some(&id), &tool.signature));
    }

    #[test]
    fn flat_geometry_is_supported_by_g21() {
        let (library, id) = library_with(ToolGeometry::Flat {
            cutting_diameter_mm: 12.0,
        });
        let resolved = resolve_cutter(&library, Some(&id), 1.0).unwrap();
        assert_eq!(resolved.diameter_model(), 12.0);
        resolved.ensure_supported().unwrap();
    }

    #[test]
    fn missing_and_invalid_scale_never_fall_back_to_manual_diameter() {
        let (lib, id) = library_with(ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        });
        assert!(resolve_cutter(&lib, None, 1.0).is_err());
        assert!(resolve_cutter(&lib, Some("deleted-id"), 1.0).is_err());
        assert!(resolve_cutter(&lib, Some(&id), f64::NAN).is_err());
        assert!(resolve_cutter(&lib, Some(&id), 0.0).is_err());
        assert!(resolve_cutter(&lib, Some(&id), f64::MAX).is_err());
    }

    #[test]
    fn unsupported_catalog_shape_is_preserved_and_rejected() {
        let (lib, id) = library_with(ToolGeometry::ConicalFlat {
            cutting_diameter_mm: 6.0,
            tip_diameter_mm: 0.1,
            included_angle_deg: 60.0,
        });
        let resolved = resolve_cutter(&lib, Some(&id), 1.0).unwrap();
        assert!(matches!(
            resolved.cutter,
            CutterGeometry::ConicalFlat { .. }
        ));
        assert!(resolved.ensure_supported().is_err());
    }

    #[test]
    fn geometry_change_and_deletion_invalidate_but_name_does_not() {
        let (mut lib, id) = library_with(ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        });
        let original = resolve_cutter(&lib, Some(&id), 1.0).unwrap().signature;
        let mut renamed = lib.get(&id).unwrap().clone();
        renamed.name = "Nombre nuevo".into();
        renamed.recommended_rpm = Some(12000);
        lib.replace(renamed).unwrap();
        assert!(signature_current(&lib, Some(&id), &original));
        let mut changed = lib.get(&id).unwrap().clone();
        changed.geometry = ToolGeometry::BallNose {
            cutting_diameter_mm: 6.0,
        };
        lib.replace(changed).unwrap();
        assert!(!signature_current(&lib, Some(&id), &original));
        lib.delete(&id).unwrap();
        assert!(!signature_current(&lib, Some(&id), &original));
    }
}

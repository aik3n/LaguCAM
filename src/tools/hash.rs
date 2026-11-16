//! Huella de geometría de fresas. No depende de JSON, identidad ni UI.
//! Contrato v1: dominio ASCII con NUL; versión u32 LE; tipo u8 estable;
//! campos geométricos f64 IEEE-754 en orden fijo y bytes little-endian.
use super::model::{Tool, ToolGeometry};
use sha2::{Digest, Sha256};

/// Versión del contrato binario de la huella, independiente de `tools.json`.
#[allow(dead_code)] // Identificador público del contrato; aún sin consumidores fuera del módulo.
pub const GEOMETRY_HASH_VERSION: u32 = 1;
const DOMAIN: &[u8] = b"LaguCAM/tool-geometry/SHA-256\0";

/// SHA-256 de la geometría física de una herramienta (32 bytes).
///
/// Orden binario v1: DOMAIN, versión `u32` LE, tipo `u8` (1 plana,
/// 2 esférica, 3 V, 4 cónica plana, 5 cónica esférica), parámetros
/// específicos del tipo en el orden del `match`, y finalmente longitud
/// de corte, diámetro de mango y longitud total, todos `f64` LE.
///
/// Solo se valida la geometría. UUID, revisión, nombre y parámetros de
/// mecanizado no intervienen ni en la comprobación ni en el digest.
/// La API no está conectada todavía a operaciones ni a trayectorias CAM.
#[allow(dead_code)] // API preparada, sin consumidor de producción en esta fase.
pub fn geometry_sha256(tool: &Tool) -> Result<[u8; 32], String> {
    tool.geometry.validate(tool.cutting_length_mm)?;
    valid_positive(tool.shank_diameter_mm, "Diámetro del mango")?;
    valid_positive(tool.total_length_mm, "Longitud total")?;
    if tool.total_length_mm < tool.cutting_length_mm {
        return Err("La longitud total no puede ser menor que la longitud de corte".into());
    }

    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(&GEOMETRY_HASH_VERSION.to_le_bytes());
    match &tool.geometry {
        ToolGeometry::Flat {
            cutting_diameter_mm,
        } => {
            bytes.push(1);
            append_f64(&mut bytes, *cutting_diameter_mm);
        }
        ToolGeometry::BallNose {
            cutting_diameter_mm,
        } => {
            bytes.push(2);
            append_f64(&mut bytes, *cutting_diameter_mm);
        }
        ToolGeometry::BullNose {
            cutting_diameter_mm,
            corner_radius_mm,
        } => {
            bytes.push(6); // Nuevo discriminante, sin alterar los cinco originales.
            append_f64(&mut bytes, *cutting_diameter_mm);
            append_f64(&mut bytes, *corner_radius_mm);
        }
        ToolGeometry::VBit {
            cutting_diameter_mm,
            included_angle_deg,
        } => {
            bytes.push(3);
            append_f64(&mut bytes, *cutting_diameter_mm);
            append_f64(&mut bytes, *included_angle_deg);
        }
        ToolGeometry::ConicalFlat {
            cutting_diameter_mm,
            tip_diameter_mm,
            included_angle_deg,
        } => {
            bytes.push(4);
            append_f64(&mut bytes, *cutting_diameter_mm);
            append_f64(&mut bytes, *tip_diameter_mm);
            append_f64(&mut bytes, *included_angle_deg);
        }
        ToolGeometry::ConicalBall {
            cutting_diameter_mm,
            tip_sphere_radius_mm,
            included_angle_deg,
        } => {
            bytes.push(5);
            append_f64(&mut bytes, *cutting_diameter_mm);
            append_f64(&mut bytes, *tip_sphere_radius_mm);
            append_f64(&mut bytes, *included_angle_deg);
        }
    }
    append_f64(&mut bytes, tool.cutting_length_mm);
    append_f64(&mut bytes, tool.shank_diameter_mm);
    append_f64(&mut bytes, tool.total_length_mm);
    let digest = Sha256::digest(&bytes);
    let mut result = [0u8; 32];
    result.copy_from_slice(&digest);
    Ok(result)
}

fn valid_positive(value: f64, what: &str) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{what}: debe ser positivo y finito"));
    }
    Ok(())
}

/// IEEE-754 binary64, LE. Normalización explícita de ambos ceros a +0.0.
/// Los NaN e infinitos no llegan aquí: se rechazan antes de codificar.
fn append_f64(bytes: &mut Vec<u8>, value: f64) {
    let bits = if value == 0.0 { 0u64 } else { value.to_bits() };
    bytes.extend_from_slice(&bits.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::model::ToolKind;

    fn sample(geometry: ToolGeometry) -> Tool {
        let mut tool = Tool::blank(ToolKind::Flat);
        tool.geometry = geometry;
        tool.cutting_length_mm = 18.0;
        tool.shank_diameter_mm = 6.0;
        tool.total_length_mm = 45.0;
        tool
    }

    fn geometries() -> [ToolGeometry; 5] {
        [
            ToolGeometry::Flat {
                cutting_diameter_mm: 6.0,
            },
            ToolGeometry::BallNose {
                cutting_diameter_mm: 6.0,
            },
            ToolGeometry::VBit {
                cutting_diameter_mm: 6.0,
                included_angle_deg: 90.0,
            },
            ToolGeometry::ConicalFlat {
                cutting_diameter_mm: 6.0,
                tip_diameter_mm: 1.0,
                included_angle_deg: 60.0,
            },
            ToolGeometry::ConicalBall {
                cutting_diameter_mm: 6.0,
                tip_sphere_radius_mm: 1.0,
                included_angle_deg: 60.0,
            },
        ]
    }

    fn hex(hash: [u8; 32]) -> String {
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn golden_sha256_vectors_for_all_five_geometries() {
        // Calculados independientemente a partir del contrato binario v1.
        let expected = [
            "c1ebed7686f550c2587e7ee568b490d1756baaef98f90f3a55ec55144799c889",
            "802447d6b18758063c5c51ff0b4d8af3425a50cc75c9ef20b319257e62852ac4",
            "1ed66eb899b58e5dd25e6982c2560304b0e2260aa8613399c5615a07c4808bef",
            "e15f03d7e3c0f712b4c369525d5cc7afc64ba4e730b9d5f0311ace49d64ed145",
            "6df7b06362bbc029ec4588cbfe8c7b780ddfba87d04c9874ce56e0580e80b40c",
        ];
        for (geometry, expected_hash) in geometries().into_iter().zip(expected) {
            assert_eq!(
                hex(geometry_sha256(&sample(geometry)).unwrap()),
                expected_hash
            );
        }
    }

    #[test]
    fn every_geometric_dimension_affects_the_hash() {
        for geometry in geometries() {
            let original = sample(geometry);
            let baseline = geometry_sha256(&original).unwrap();
            for dimension in 0..3 {
                let mut modified = original.clone();
                match dimension {
                    0 => modified.cutting_length_mm += 1.0,
                    1 => modified.shank_diameter_mm += 0.5,
                    _ => modified.total_length_mm += 1.0,
                }
                assert_ne!(geometry_sha256(&modified).unwrap(), baseline);
            }
            let mut changed = original.clone();
            match &mut changed.geometry {
                ToolGeometry::Flat {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BallNose {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BullNose {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::VBit {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalFlat {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalBall {
                    cutting_diameter_mm,
                    ..
                } => *cutting_diameter_mm += 0.5,
            }
            assert_ne!(geometry_sha256(&changed).unwrap(), baseline);
            let mut changed_angle = original.clone();
            match &mut changed_angle.geometry {
                ToolGeometry::VBit {
                    included_angle_deg, ..
                }
                | ToolGeometry::ConicalFlat {
                    included_angle_deg, ..
                }
                | ToolGeometry::ConicalBall {
                    included_angle_deg, ..
                } => {
                    *included_angle_deg += 5.0;
                    assert_ne!(geometry_sha256(&changed_angle).unwrap(), baseline);
                }
                _ => {}
            }
            let mut changed_tip = original.clone();
            match &mut changed_tip.geometry {
                ToolGeometry::ConicalFlat {
                    tip_diameter_mm, ..
                } => *tip_diameter_mm += 0.25,
                ToolGeometry::ConicalBall {
                    tip_sphere_radius_mm,
                    ..
                } => *tip_sphere_radius_mm += 0.25,
                _ => continue,
            }
            assert_ne!(geometry_sha256(&changed_tip).unwrap(), baseline);
        }
    }

    #[test]
    fn non_geometric_fields_do_not_affect_the_hash() {
        for geometry in geometries() {
            let tool = sample(geometry);
            let baseline = geometry_sha256(&tool).unwrap();
            let mut edited = tool.clone();
            edited.id = "bb2c68b0-bbbb-4000-8888-000000000001".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.revision = 999;
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.name = "Otro nombre".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.description = "Otra descripción".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.commercial_reference = "ABC-123".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.flute_count = Some(3);
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.recommended_rpm = Some(12_000);
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.recommended_feed_mm_min = Some(720.0);
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.recommended_material = "Aluminio".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
            edited.notes = "Notas diferentes".into();
            assert_eq!(geometry_sha256(&edited).unwrap(), baseline);
        }
    }

    #[test]
    fn diameter_six_to_four_to_six_recovers_original_hash() {
        for geometry in geometries() {
            let mut tool = sample(geometry);
            tool.id = "8a4aab11-8888-4000-9000-000000000001".into();
            tool.revision = 1;
            let original = geometry_sha256(&tool).unwrap();
            match &mut tool.geometry {
                ToolGeometry::Flat {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BallNose {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BullNose {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::VBit {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalFlat {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalBall {
                    cutting_diameter_mm,
                    ..
                } => *cutting_diameter_mm = 4.0,
            }
            tool.revision = 2;
            assert_ne!(geometry_sha256(&tool).unwrap(), original);
            match &mut tool.geometry {
                ToolGeometry::Flat {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BallNose {
                    cutting_diameter_mm,
                }
                | ToolGeometry::BullNose {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::VBit {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalFlat {
                    cutting_diameter_mm,
                    ..
                }
                | ToolGeometry::ConicalBall {
                    cutting_diameter_mm,
                    ..
                } => *cutting_diameter_mm = 6.0,
            }
            tool.revision = 3;
            assert_eq!(geometry_sha256(&tool).unwrap(), original);
        }
    }

    #[test]
    fn bull_hash_changes_with_corner_but_not_name() {
        let mut tool = sample(ToolGeometry::BullNose {
            cutting_diameter_mm: 12.0,
            corner_radius_mm: 1.0,
        });
        let baseline = geometry_sha256(&tool).unwrap();
        tool.name = "Otra etiqueta".into();
        assert_eq!(geometry_sha256(&tool).unwrap(), baseline);
        tool.geometry = ToolGeometry::BullNose {
            cutting_diameter_mm: 12.0,
            corner_radius_mm: 2.0,
        };
        assert_ne!(geometry_sha256(&tool).unwrap(), baseline);
    }

    #[test]
    fn invalid_dimensions_are_rejected_without_hashing() {
        let mut tool = sample(geometries()[0].clone());
        tool.shank_diameter_mm = f64::INFINITY;
        assert!(geometry_sha256(&tool).is_err());
        tool.shank_diameter_mm = 6.0;
        tool.total_length_mm = 1.0;
        assert!(geometry_sha256(&tool).is_err());
        tool.total_length_mm = 45.0;
        tool.cutting_length_mm = f64::NAN;
        assert!(geometry_sha256(&tool).is_err());
        tool.cutting_length_mm = 18.0;
        tool.geometry = ToolGeometry::ConicalBall {
            cutting_diameter_mm: 6.0,
            tip_sphere_radius_mm: f64::NAN,
            included_angle_deg: 60.0,
        };
        assert!(geometry_sha256(&tool).is_err());
    }

    #[test]
    fn binary64_encoding_is_le_and_normalizes_signed_zero() {
        let mut plus = Vec::new();
        append_f64(&mut plus, 0.0);
        let mut minus = Vec::new();
        append_f64(&mut minus, -0.0);
        assert_eq!(plus, minus);
        assert_eq!(plus, [0u8; 8]);
        let mut value = Vec::new();
        append_f64(&mut value, 1.0);
        assert_eq!(value, [0, 0, 0, 0, 0, 0, 240, 63]);
    }
}

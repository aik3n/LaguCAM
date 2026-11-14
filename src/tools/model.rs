//! Datos y geometría de fresas. Sin dependencia de egui ni de OpenGL.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

pub const LIBRARY_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Flat,
    BallNose,
    VBit,
    ConicalFlat,
    ConicalBall,
}

impl ToolKind {
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::BallNose,
        Self::VBit,
        Self::ConicalFlat,
        Self::ConicalBall,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flat => "Fresa plana",
            Self::BallNose => "Fresa esférica",
            Self::VBit => "Cónica en V",
            Self::ConicalFlat => "Cónica, punta plana",
            Self::ConicalBall => "Cónica, punta esférica",
        }
    }
}

/// Los ángulos cónicos son ángulos INCLUIDOS (totales), en grados.
/// La punta esférica es un casquete de esfera tangente al flanco del cono;
/// NO representa un radio toroidal ni una punta plana con bordes redondeados.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolGeometry {
    Flat {
        cutting_diameter_mm: f64,
    },
    BallNose {
        cutting_diameter_mm: f64,
    },
    VBit {
        cutting_diameter_mm: f64,
        included_angle_deg: f64,
    },
    ConicalFlat {
        cutting_diameter_mm: f64,
        tip_diameter_mm: f64,
        included_angle_deg: f64,
    },
    ConicalBall {
        cutting_diameter_mm: f64,
        tip_sphere_radius_mm: f64,
        included_angle_deg: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfilePoint {
    pub radius_mm: f64,
    /// Distancia desde el extremo axial más bajo de la fresa hacia el mango.
    pub z_mm: f64,
}

fn positive(value: f64, what: &str) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{what}: debe ser positivo y finito"));
    }
    Ok(())
}

fn angle_radians(degrees: f64) -> Result<f64, String> {
    if !degrees.is_finite() || degrees <= 0.0 || degrees >= 180.0 {
        return Err("Ángulo incluido: debe estar estrictamente entre 0° y 180°".into());
    }
    Ok(degrees.to_radians() * 0.5)
}

impl ToolGeometry {
    pub const fn kind(&self) -> ToolKind {
        match self {
            Self::Flat { .. } => ToolKind::Flat,
            Self::BallNose { .. } => ToolKind::BallNose,
            Self::VBit { .. } => ToolKind::VBit,
            Self::ConicalFlat { .. } => ToolKind::ConicalFlat,
            Self::ConicalBall { .. } => ToolKind::ConicalBall,
        }
    }

    /// Un formulario nuevo comienza sin dimensiones ficticias; hay que introducirlas.
    pub const fn blank(kind: ToolKind) -> Self {
        match kind {
            ToolKind::Flat => Self::Flat {
                cutting_diameter_mm: 0.0,
            },
            ToolKind::BallNose => Self::BallNose {
                cutting_diameter_mm: 0.0,
            },
            ToolKind::VBit => Self::VBit {
                cutting_diameter_mm: 0.0,
                included_angle_deg: 0.0,
            },
            ToolKind::ConicalFlat => Self::ConicalFlat {
                cutting_diameter_mm: 0.0,
                tip_diameter_mm: 0.0,
                included_angle_deg: 0.0,
            },
            ToolKind::ConicalBall => Self::ConicalBall {
                cutting_diameter_mm: 0.0,
                tip_sphere_radius_mm: 0.0,
                included_angle_deg: 0.0,
            },
        }
    }

    pub const fn diameter_mm(&self) -> f64 {
        match self {
            Self::Flat {
                cutting_diameter_mm,
            }
            | Self::BallNose {
                cutting_diameter_mm,
            }
            | Self::VBit {
                cutting_diameter_mm,
                ..
            }
            | Self::ConicalFlat {
                cutting_diameter_mm,
                ..
            }
            | Self::ConicalBall {
                cutting_diameter_mm,
                ..
            } => *cutting_diameter_mm,
        }
    }

    /// Longitud axial mínima de la punta hasta alcanzar el diámetro de corte indicado.
    /// Si la longitud cortante es mayor, continúa una sección cilíndrica cortante.
    pub fn tip_height_mm(&self) -> Result<f64, String> {
        let d = self.diameter_mm();
        positive(d, "Diámetro de corte")?;
        let height = match self {
            Self::Flat { .. } => 0.0,
            Self::BallNose { .. } => d * 0.5,
            Self::VBit {
                included_angle_deg, ..
            } => d * 0.5 / angle_radians(*included_angle_deg)?.tan(),
            Self::ConicalFlat {
                tip_diameter_mm,
                included_angle_deg,
                ..
            } => {
                positive(*tip_diameter_mm, "Diámetro de la punta plana")?;
                if *tip_diameter_mm >= d {
                    return Err("La punta plana debe ser menor que el diámetro de corte".into());
                }
                (d - tip_diameter_mm) * 0.5 / angle_radians(*included_angle_deg)?.tan()
            }
            Self::ConicalBall {
                tip_sphere_radius_mm,
                included_angle_deg,
                ..
            } => {
                positive(*tip_sphere_radius_mm, "Radio de la esfera de punta")?;
                let alpha = angle_radians(*included_angle_deg)?;
                let tangent_radius = tip_sphere_radius_mm * alpha.cos();
                if d * 0.5 <= tangent_radius {
                    return Err(
                        "El diámetro de corte debe superar el diámetro en la tangencia esférica"
                            .into(),
                    );
                }
                let tangent_z = tip_sphere_radius_mm * (1.0 - alpha.sin());
                tangent_z + (d * 0.5 - tangent_radius) / alpha.tan()
            }
        };
        if !height.is_finite() || height < 0.0 {
            return Err("La punta excede el rango geométrico f64".into());
        }
        Ok(height)
    }

    pub fn validate(&self, cutting_length_mm: f64) -> Result<(), String> {
        positive(cutting_length_mm, "Longitud de corte")?;
        let height = self.tip_height_mm()?;
        if cutting_length_mm < height {
            return Err(format!(
                "Longitud de corte insuficiente: la punta necesita {height:.6} mm"
            ));
        }
        Ok(())
    }

    /// Perfil radial derecho en coordenadas exactas f64; el dibujo solo proyecta.
    pub fn profile(&self, cutting_length_mm: f64) -> Result<Vec<ProfilePoint>, String> {
        self.validate(cutting_length_mm)?;
        let mut points = Vec::with_capacity(36);
        let height = self.tip_height_mm()?;
        let max_radius = self.diameter_mm() * 0.5;
        match self {
            Self::Flat { .. } => {
                points.push(ProfilePoint {
                    radius_mm: max_radius,
                    z_mm: 0.0,
                });
            }
            Self::BallNose { .. } => {
                for i in 0..=24 {
                    let t = std::f64::consts::FRAC_PI_2 * f64::from(i) / 24.0;
                    points.push(ProfilePoint {
                        radius_mm: max_radius * t.sin(),
                        z_mm: max_radius * (1.0 - t.cos()),
                    });
                }
            }
            Self::VBit { .. } => {
                points.push(ProfilePoint {
                    radius_mm: 0.0,
                    z_mm: 0.0,
                });
                points.push(ProfilePoint {
                    radius_mm: max_radius,
                    z_mm: height,
                });
            }
            Self::ConicalFlat {
                tip_diameter_mm, ..
            } => {
                points.push(ProfilePoint {
                    radius_mm: tip_diameter_mm * 0.5,
                    z_mm: 0.0,
                });
                points.push(ProfilePoint {
                    radius_mm: max_radius,
                    z_mm: height,
                });
            }
            Self::ConicalBall {
                tip_sphere_radius_mm,
                included_angle_deg,
                ..
            } => {
                let alpha = angle_radians(*included_angle_deg)?;
                // Parámetro de círculo: r=R sin(t), z=R(1-cos(t)).
                // En la tangencia: t=π/2-α, flanco dr/dz=tan(α).
                let tangent_t = std::f64::consts::FRAC_PI_2 - alpha;
                for i in 0..=24 {
                    let t = tangent_t * f64::from(i) / 24.0;
                    points.push(ProfilePoint {
                        radius_mm: tip_sphere_radius_mm * t.sin(),
                        z_mm: tip_sphere_radius_mm * (1.0 - t.cos()),
                    });
                }
                points.push(ProfilePoint {
                    radius_mm: max_radius,
                    z_mm: height,
                });
            }
        }
        if cutting_length_mm > height {
            points.push(ProfilePoint {
                radius_mm: max_radius,
                z_mm: cutting_length_mm,
            });
        }
        Ok(points)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub geometry: ToolGeometry,
    pub cutting_length_mm: f64,
    pub shank_diameter_mm: f64,
    /// Longitud axial desde la punta hasta el extremo superior del mango representado.
    /// No equivale a salida de pinza o longitud segura de mecanizado.
    #[serde(alias = "free_length_mm")]
    pub total_length_mm: f64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commercial_reference: String,
    #[serde(default)]
    pub flute_count: Option<u32>,
    #[serde(default)]
    pub recommended_rpm: Option<u32>,
    #[serde(default)]
    pub recommended_feed_mm_min: Option<f64>,
    #[serde(default)]
    pub recommended_material: String,
    #[serde(default)]
    pub notes: String,
}

impl Tool {
    pub fn blank(kind: ToolKind) -> Self {
        Self {
            id: String::new(),
            revision: 0,
            name: String::new(),
            geometry: ToolGeometry::blank(kind),
            cutting_length_mm: 0.0,
            shank_diameter_mm: 0.0,
            total_length_mm: 0.0,
            description: String::new(),
            commercial_reference: String::new(),
            flute_count: None,
            recommended_rpm: None,
            recommended_feed_mm_min: None,
            recommended_material: String::new(),
            notes: String::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if Uuid::parse_str(&self.id).is_err() {
            return Err("ID de herramienta inválido".into());
        }
        if self.revision == 0 {
            return Err("La revisión debe ser mayor que cero".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err("Nombre de herramienta vacío o demasiado largo".into());
        }
        for text in [
            &self.description,
            &self.commercial_reference,
            &self.recommended_material,
            &self.notes,
        ] {
            if text.len() > 8192 {
                return Err("Campo de texto de herramienta demasiado largo".into());
            }
        }
        positive(self.shank_diameter_mm, "Diámetro del mango")?;
        positive(self.total_length_mm, "Longitud total")?;
        self.geometry.validate(self.cutting_length_mm)?;
        if self.total_length_mm < self.cutting_length_mm {
            return Err("La longitud total no puede ser menor que la longitud de corte".into());
        }
        if matches!(self.flute_count, Some(0)) || matches!(self.recommended_rpm, Some(0)) {
            return Err(
                "Filos/RPM recomendadas deben ser mayores que cero, o estar sin definir".into(),
            );
        }
        if let Some(feed) = self.recommended_feed_mm_min {
            positive(feed, "Avance recomendado")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolLibrary {
    pub format: String,
    pub version: u32,
    pub tools: Vec<Tool>,
}

impl Default for ToolLibrary {
    fn default() -> Self {
        Self {
            format: "LaguCAM-tools".into(),
            version: LIBRARY_VERSION,
            tools: Vec::new(),
        }
    }
}

impl ToolLibrary {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "LaguCAM-tools" || self.version != LIBRARY_VERSION {
            return Err("Formato o versión de biblioteca incompatible".into());
        }
        if self.tools.len() > 100_000 {
            return Err("Biblioteca demasiado grande".into());
        }
        let mut ids = HashSet::new();
        for tool in &self.tools {
            tool.validate()?;
            if !ids.insert(tool.id.as_str()) {
                return Err("ID de herramienta duplicado".into());
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.id == id)
    }

    pub fn add(&mut self, mut draft: Tool) -> Result<String, String> {
        if !draft.id.is_empty() || draft.revision != 0 {
            return Err("Una herramienta nueva no debe reutilizar una identidad".into());
        }
        draft.id = Uuid::new_v4().to_string();
        draft.revision = 1;
        draft.validate()?;
        if self.get(&draft.id).is_some() {
            return Err("Colisión de ID; vuelve a intentar".into());
        }
        let id = draft.id.clone();
        self.tools.push(draft);
        Ok(id)
    }

    pub fn replace(&mut self, mut updated: Tool) -> Result<(), String> {
        let previous = self.get(&updated.id).ok_or("Herramienta inexistente")?;
        updated.revision = previous.revision.checked_add(1).ok_or("Revisión agotada")?;
        updated.validate()?;
        let index = self
            .tools
            .iter()
            .position(|t| t.id == updated.id)
            .ok_or("Herramienta inexistente")?;
        self.tools[index] = updated;
        Ok(())
    }

    pub fn duplicate(&self, id: &str) -> Result<Tool, String> {
        let mut copy = self.get(id).ok_or("Herramienta inexistente")?.clone();
        copy.id.clear();
        copy.revision = 0;
        copy.name = format!("Copia de {}", copy.name);
        Ok(copy)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .tools
            .iter()
            .position(|t| t.id == id)
            .ok_or("Herramienta inexistente")?;
        self.tools.remove(index);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSort {
    Name,
    Kind,
    Diameter,
}

impl ToolSort {
    pub const ALL: [Self; 3] = [Self::Name, Self::Kind, Self::Diameter];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Nombre",
            Self::Kind => "Tipo",
            Self::Diameter => "Diámetro",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolQuery {
    pub search: String,
    pub kind: Option<ToolKind>,
    pub min_diameter_mm: f64,
    pub max_diameter_mm: Option<f64>,
    pub sort: ToolSort,
}

impl Default for ToolQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            kind: None,
            min_diameter_mm: 0.0,
            max_diameter_mm: None,
            sort: ToolSort::Name,
        }
    }
}

impl ToolQuery {
    pub fn filtered<'a>(&self, lib: &'a ToolLibrary) -> Vec<&'a Tool> {
        let needle = self.search.trim().to_lowercase();
        let mut list: Vec<_> = lib
            .tools
            .iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&needle)
                    && self.kind.is_none_or(|kind| t.geometry.kind() == kind)
                    && t.geometry.diameter_mm() >= self.min_diameter_mm
                    && self
                        .max_diameter_mm
                        .is_none_or(|max| t.geometry.diameter_mm() <= max)
            })
            .collect();
        list.sort_by(|a, b| {
            let order = match self.sort {
                ToolSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ToolSort::Kind => (a.geometry.kind() as u8).cmp(&(b.geometry.kind() as u8)),
                ToolSort::Diameter => a
                    .geometry
                    .diameter_mm()
                    .total_cmp(&b.geometry.diameter_mm()),
            };
            order.then_with(|| a.id.cmp(&b.id))
        });
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(kind: ToolGeometry) -> Tool {
        Tool {
            id: Uuid::new_v4().to_string(),
            revision: 1,
            name: "Test".into(),
            geometry: kind,
            cutting_length_mm: 20.0,
            shank_diameter_mm: 6.0,
            total_length_mm: 30.0,
            description: String::new(),
            commercial_reference: String::new(),
            flute_count: None,
            recommended_rpm: None,
            recommended_feed_mm_min: None,
            recommended_material: String::new(),
            notes: String::new(),
        }
    }

    #[test]
    fn flat_ball_and_v_profiles() {
        let flat = tool(ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        });
        assert!(flat.validate().is_ok());
        assert_eq!(flat.geometry.profile(20.0).unwrap()[0].radius_mm, 3.0);
        let ball = tool(ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        });
        assert_eq!(ball.geometry.tip_height_mm().unwrap(), 2.0);
        assert_eq!(ball.geometry.profile(20.0).unwrap()[0].radius_mm, 0.0);
        let v = ToolGeometry::VBit {
            cutting_diameter_mm: 6.0,
            included_angle_deg: 90.0,
        };
        assert!((v.tip_height_mm().unwrap() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn cone_flat_and_spherical_tangent_are_distinct() {
        let flat = ToolGeometry::ConicalFlat {
            cutting_diameter_mm: 6.0,
            tip_diameter_mm: 1.0,
            included_angle_deg: 90.0,
        };
        assert_eq!(flat.profile(20.0).unwrap()[0].radius_mm, 0.5);
        let ball = ToolGeometry::ConicalBall {
            cutting_diameter_mm: 6.0,
            tip_sphere_radius_mm: 1.0,
            included_angle_deg: 90.0,
        };
        assert_eq!(ball.profile(20.0).unwrap()[0].radius_mm, 0.0);
        let profile = ball.profile(20.0).unwrap();
        let tangent = profile[24];
        assert!((tangent.radius_mm - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((tangent.z_mm - (1.0 - std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-12);
    }

    #[test]
    fn rejects_nonfinite_and_inconsistent_geometry() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
            assert!(
                ToolGeometry::Flat {
                    cutting_diameter_mm: invalid
                }
                .validate(5.0)
                .is_err()
            );
        }
        assert!(
            ToolGeometry::VBit {
                cutting_diameter_mm: 6.0,
                included_angle_deg: 0.0
            }
            .validate(5.0)
            .is_err()
        );
        assert!(
            ToolGeometry::VBit {
                cutting_diameter_mm: 6.0,
                included_angle_deg: 180.0
            }
            .validate(5.0)
            .is_err()
        );
        assert!(
            ToolGeometry::ConicalFlat {
                cutting_diameter_mm: 6.0,
                tip_diameter_mm: 7.0,
                included_angle_deg: 90.0
            }
            .validate(5.0)
            .is_err()
        );
        assert!(
            ToolGeometry::ConicalBall {
                cutting_diameter_mm: 1.0,
                tip_sphere_radius_mm: 2.0,
                included_angle_deg: 90.0
            }
            .validate(5.0)
            .is_err()
        );
        assert!(
            ToolGeometry::BallNose {
                cutting_diameter_mm: 6.0
            }
            .validate(2.0)
            .is_err()
        );
        let mut t = tool(ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        });
        t.total_length_mm = 10.0;
        assert!(t.validate().is_err());
        t.total_length_mm = 30.0;
        t.recommended_feed_mm_min = Some(f64::NAN);
        assert!(t.validate().is_err());
        t.recommended_feed_mm_min = None;
        t.recommended_rpm = Some(0);
        assert!(t.validate().is_err());
    }

    #[test]
    fn stable_id_and_revision_on_edit_duplicate_delete() {
        let mut lib = ToolLibrary::default();
        let mut new = Tool::blank(ToolKind::Flat);
        new.name = "Fresa Ø6".into();
        new.geometry = ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        };
        new.cutting_length_mm = 12.0;
        new.shank_diameter_mm = 6.0;
        new.total_length_mm = 35.0;
        let id = lib.add(new).unwrap();
        let mut edited = lib.get(&id).unwrap().clone();
        edited.name = "Fresa Ø6 editada".into();
        lib.replace(edited).unwrap();
        assert_eq!(lib.get(&id).unwrap().revision, 2);
        let dup = lib.duplicate(&id).unwrap();
        let copy_id = lib.add(dup).unwrap();
        assert_ne!(id, copy_id);
        assert_eq!(lib.get(&copy_id).unwrap().revision, 1);
        lib.delete(&id).unwrap();
        assert!(lib.get(&id).is_none());
    }

    #[test]
    fn filters_and_sorts() {
        let mut lib = ToolLibrary::default();
        for (name, diameter, kind) in [
            ("Z", 8.0, ToolKind::Flat),
            ("A", 4.0, ToolKind::BallNose),
            ("C", 6.0, ToolKind::Flat),
        ] {
            let mut t = Tool::blank(kind);
            t.name = name.into();
            t.geometry = match kind {
                ToolKind::Flat => ToolGeometry::Flat {
                    cutting_diameter_mm: diameter,
                },
                _ => ToolGeometry::BallNose {
                    cutting_diameter_mm: diameter,
                },
            };
            t.cutting_length_mm = 20.0;
            t.shank_diameter_mm = 8.0;
            t.total_length_mm = 40.0;
            lib.add(t).unwrap();
        }
        let q = ToolQuery {
            min_diameter_mm: 5.0,
            max_diameter_mm: Some(8.0),
            sort: ToolSort::Diameter,
            ..Default::default()
        };
        let matches = q.filtered(&lib);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].name, "C");
        let q = ToolQuery {
            search: "a".into(),
            ..Default::default()
        };
        assert_eq!(q.filtered(&lib).len(), 1);
        let q = ToolQuery {
            kind: Some(ToolKind::Flat),
            ..Default::default()
        };
        assert_eq!(q.filtered(&lib).len(), 2);
    }

    #[test]
    fn serialization_preserves_f64_bits_and_ids() {
        let mut lib = ToolLibrary::default();
        let mut t = tool(ToolGeometry::ConicalBall {
            cutting_diameter_mm: 6.1234567890123,
            tip_sphere_radius_mm: 0.123456789012345,
            included_angle_deg: 33.333333333333,
        });
        t.name = "Precisión".into();
        lib.tools.push(t);
        let data = serde_json::to_string_pretty(&lib).unwrap();
        let restored: ToolLibrary = serde_json::from_str(&data).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.tools[0], lib.tools[0]);
        match (&restored.tools[0].geometry, &lib.tools[0].geometry) {
            (
                ToolGeometry::ConicalBall {
                    cutting_diameter_mm,
                    tip_sphere_radius_mm,
                    included_angle_deg,
                },
                ToolGeometry::ConicalBall {
                    cutting_diameter_mm: a,
                    tip_sphere_radius_mm: b,
                    included_angle_deg: c,
                },
            ) => {
                assert_eq!(cutting_diameter_mm.to_bits(), a.to_bits());
                assert_eq!(tip_sphere_radius_mm.to_bits(), b.to_bits());
                assert_eq!(included_angle_deg.to_bits(), c.to_bits());
            }
            _ => panic!("Se ha alterado el tipo durante la serialización"),
        }
        let mut tampered = restored;
        tampered.tools.push(tampered.tools[0].clone());
        assert!(tampered.validate().is_err());
    }
}

#[cfg(test)]
mod total_length_compatibility_tests {
    use super::*;

    #[test]
    fn reads_legacy_free_length_and_writes_total_length_without_numeric_change() {
        let mut tool = Tool::blank(ToolKind::Flat);
        tool.id = Uuid::new_v4().to_string();
        tool.revision = 1;
        tool.name = "Antigua".into();
        tool.geometry = ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        };
        tool.cutting_length_mm = 18.0;
        tool.shank_diameter_mm = 6.0;
        tool.total_length_mm = 40.125;
        let mut doc = serde_json::to_value(&tool).unwrap();
        let total = doc
            .as_object_mut()
            .unwrap()
            .remove("total_length_mm")
            .unwrap();
        doc["free_length_mm"] = total;
        let loaded: Tool = serde_json::from_value(doc).unwrap();
        assert_eq!(loaded.total_length_mm.to_bits(), 40.125_f64.to_bits());
        assert!(loaded.validate().is_ok());
        let saved = serde_json::to_string(&loaded).unwrap();
        assert!(saved.contains("\"total_length_mm\""));
        assert!(!saved.contains("\"free_length_mm\""));
    }
}

//! P1-M2: unidad de origen y transformacion no destructiva del STL.
//!
//! El archivo STL sigue intacto. La misma matriz alimenta visor y CAM en mm.

use crate::stl::Bounds;
use cam_libre_visor::cam_geometry::stl::GeometryAffine;
use glam::{DMat3, DVec3, Mat4};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StlSourceUnit {
    #[default]
    #[serde(rename = "mm")]
    Millimeters,
    #[serde(rename = "inch")]
    Inches,
}

impl StlSourceUnit {
    pub const fn mm_per_unit(self) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Inches => 25.4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPivot {
    #[default]
    StlOrigin,
    BoundsCenter,
}

/// Escala adicional SIN unidades, rotacion en grados, traslacion en mm.
/// No se permiten reflejos ni escalas nulas/negativas en este hito.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTransform {
    pub source_unit: StlSourceUnit,
    pub scale_xyz: [f64; 3],
    pub rotation_degrees: [f64; 3],
    pub translation_mm: [f64; 3],
    pub pivot: ModelPivot,
}

impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            source_unit: StlSourceUnit::Millimeters,
            scale_xyz: [1.0; 3],
            rotation_degrees: [0.0; 3],
            translation_mm: [0.0; 3],
            pivot: ModelPivot::StlOrigin,
        }
    }
}

impl ModelTransform {
    pub fn is_identity(&self) -> bool {
        self == &Self::default()
    }

    /// Construccion exacta de matriz desde el STL original (sin acumulacion de cambios).
    pub fn affine(&self, raw_bounds: &Bounds) -> Result<GeometryAffine, String> {
        self.validate()?;
        for (min, max) in raw_bounds.min.iter().zip(raw_bounds.max.iter()) {
            if !min.is_finite() || !max.is_finite() || max < min || !(max - min).is_finite() {
                return Err("Limites originales STL invalidos".into());
            }
        }
        let unit = self.source_unit.mm_per_unit();
        let [rx, ry, rz] = self.rotation_degrees.map(f64::to_radians);
        let rotation =
            DMat3::from_rotation_z(rz) * DMat3::from_rotation_y(ry) * DMat3::from_rotation_x(rx);
        let linear =
            rotation * DMat3::from_diagonal(DVec3::from_array(self.scale_xyz.map(|x| x * unit)));
        let pivot = match self.pivot {
            ModelPivot::StlOrigin => DVec3::ZERO,
            ModelPivot::BoundsCenter => DVec3::from_array(raw_bounds.center()),
        };
        let offset = pivot * unit - linear * pivot + DVec3::from_array(self.translation_mm);
        let result = GeometryAffine {
            columns: [
                linear.x_axis.to_array(),
                linear.y_axis.to_array(),
                linear.z_axis.to_array(),
            ],
            offset: offset.to_array(),
        };
        if !result.is_finite() {
            return Err("Transformacion STL no representable".into());
        }
        Ok(result)
    }

    /// AABB real del modelo; transforma las ocho esquinas y comprueba rango.
    pub fn transformed_bounds(&self, raw: &Bounds) -> Result<Bounds, String> {
        let affine = self.affine(raw)?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for bits in 0..8 {
            let p = std::array::from_fn(|i| {
                if bits & (1 << i) == 0 {
                    raw.min[i]
                } else {
                    raw.max[i]
                }
            });
            let v = affine
                .transform_point(p)
                .map_err(|_| "Limites transformados fuera de rango")?;
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        if min.iter().zip(max).any(|(lo, hi)| !(hi - lo).is_finite()) {
            return Err("Extension transformada fuera de rango".into());
        }
        Ok(Bounds { min, max })
    }

    /// La malla GPU contiene coordenadas originales normalizadas a [-1, 1].
    /// Esta matriz las convierte en las coordenadas normalizadas del MODELO real.
    pub fn preview_matrix(&self, raw: &Bounds, actual: &Bounds) -> Result<Mat4, String> {
        let a = self.affine(raw)?;
        let input_span = raw.dimensions().into_iter().fold(0.0_f64, f64::max);
        let output_span = actual.dimensions().into_iter().fold(0.0_f64, f64::max);
        if !input_span.is_finite()
            || !output_span.is_finite()
            || input_span <= 0.0
            || output_span <= 0.0
        {
            return Err("El modelo no tiene extension visual valida".into());
        }
        let raw_center = raw.center();
        let center = actual.center();
        let mapped_center = a
            .transform_point(raw_center)
            .map_err(|_| "Centro del STL transformado fuera de rango")?;
        let columns: [[f64; 3]; 3] = a.columns.map(|c| c.map(|x| x * input_span / output_span));
        let shift: [f64; 3] =
            std::array::from_fn(|i| (mapped_center[i] - center[i]) * 2.0 / output_span);
        let all = [
            columns[0][0],
            columns[0][1],
            columns[0][2],
            0.0,
            columns[1][0],
            columns[1][1],
            columns[1][2],
            0.0,
            columns[2][0],
            columns[2][1],
            columns[2][2],
            0.0,
            shift[0],
            shift[1],
            shift[2],
            1.0,
        ];
        if all
            .iter()
            .any(|v| !v.is_finite() || v.abs() > f64::from(f32::MAX))
        {
            return Err("Matriz de previsualizacion fuera de rango".into());
        }
        let matrix = Mat4::from_cols_array(&all.map(|x| x as f32));
        if !matrix.determinant().is_finite() || matrix.determinant() == 0.0 {
            return Err("Transformacion visual demasiado extrema".into());
        }
        Ok(matrix)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.scale_xyz.iter().all(|x| x.is_finite() && *x > 0.0) {
            return Err("Escala STL: los tres factores deben ser positivos y finitos".into());
        }
        if !self.rotation_degrees.iter().all(|x| x.is_finite()) {
            return Err("Rotacion STL: los tres angulos deben ser finitos".into());
        }
        if !self.translation_mm.iter().all(|x| x.is_finite()) {
            return Err("Traslacion STL: los tres valores deben ser finitos".into());
        }
        Ok(())
    }

    /// Convierte un punto STL original a mm con el convenio:
    /// T_mm + P_mm + Rz * Ry * Rx * S_xyz * ((p_stl - P_stl) * unidad_mm).
    /// No muta ni el punto de origen ni el archivo STL.
    pub fn transform_point(&self, raw: [f64; 3], raw_bounds: &Bounds) -> Result<[f64; 3], String> {
        self.validate()?;
        if !raw.iter().all(|x| x.is_finite())
            || !raw_bounds.min.iter().all(|x| x.is_finite())
            || !raw_bounds.max.iter().all(|x| x.is_finite())
            || !raw_bounds
                .min
                .iter()
                .zip(raw_bounds.max)
                .all(|(min, max)| max >= *min && (max - min).is_finite())
        {
            return Err("Coordenadas o limites STL no validos".into());
        }
        let pivot = match self.pivot {
            ModelPivot::StlOrigin => [0.0; 3],
            ModelPivot::BoundsCenter => raw_bounds.center(),
        };
        let unit = self.source_unit.mm_per_unit();
        let p =
            std::array::from_fn::<f64, 3, _>(|i| ((raw[i] - pivot[i]) * unit) * self.scale_xyz[i]);

        let (sx, cx) = self.rotation_degrees[0].to_radians().sin_cos();
        let (sy, cy) = self.rotation_degrees[1].to_radians().sin_cos();
        let (sz, cz) = self.rotation_degrees[2].to_radians().sin_cos();
        let x = [p[0], cx * p[1] - sx * p[2], sx * p[1] + cx * p[2]];
        let y = [cy * x[0] + sy * x[2], x[1], -sy * x[0] + cy * x[2]];
        let z = [cz * y[0] - sz * y[1], sz * y[0] + cz * y[1], y[2]];
        let result =
            std::array::from_fn::<f64, 3, _>(|i| pivot[i] * unit + z[i] + self.translation_mm[i]);
        if !result.iter().all(|x| x.is_finite()) {
            return Err("La transformacion STL desborda los limites numericos".into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Project, persistence::ProjectState};

    fn bounds() -> Bounds {
        Bounds {
            min: [0.0; 3],
            max: [2.0; 3],
        }
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn defaults_are_identity_and_original_projects_keep_exact_json_shape() {
        let transform = ModelTransform::default();
        assert!(transform.is_identity());
        assert_eq!(
            transform
                .transform_point([2.0, 1.0, 0.0], &bounds())
                .unwrap(),
            [2.0, 1.0, 0.0]
        );
        let state = ProjectState::capture(&Project::default());
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("model_transform"));
        let loaded: ProjectState = serde_json::from_str(&json).unwrap();
        assert!(loaded.model_transform.is_identity());
        assert_eq!(loaded, state);
    }

    #[test]
    fn inch_is_interpreted_as_25_point_4_mm_without_changing_working_units() {
        let transform = ModelTransform {
            source_unit: StlSourceUnit::Inches,
            ..Default::default()
        };
        let raw = [2.0, 1.0, 0.0];
        let point = transform.transform_point(raw, &bounds()).unwrap();
        close(point[0], 50.8);
        close(point[1], 25.4);
        assert_eq!(raw, [2.0, 1.0, 0.0]);
    }

    #[test]
    fn independent_scale_rotation_translation_and_both_pivots() {
        let mut transform = ModelTransform {
            scale_xyz: [2.0, 3.0, 4.0],
            rotation_degrees: [0.0, 0.0, 90.0],
            translation_mm: [10.0, -5.0, 1.0],
            ..Default::default()
        };
        let p = transform
            .transform_point([1.0, 2.0, 1.0], &bounds())
            .unwrap();
        close(p[0], 4.0);
        close(p[1], -3.0);
        close(p[2], 5.0);
        transform.pivot = ModelPivot::BoundsCenter;
        let p = transform
            .transform_point([2.0, 1.0, 1.0], &bounds())
            .unwrap();
        close(p[0], 11.0);
        close(p[1], -2.0);
        close(p[2], 2.0);
    }

    #[test]
    fn nonidentity_serializes_but_is_blocked_until_viewer_and_cam_are_integrated() {
        let mut state = ProjectState::capture(&Project::default());
        state.model_transform.source_unit = StlSourceUnit::Inches;
        let encoded = serde_json::to_string(&state).unwrap();
        assert!(encoded.contains("\"inch\""));
        let decoded: ProjectState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, state);
        assert!(decoded.validate(None).is_err());
    }

    #[test]
    fn affine_and_bounds_match_transformed_points() {
        let mut t = ModelTransform {
            source_unit: StlSourceUnit::Inches,
            scale_xyz: [2.0, 1.0, 3.0],
            rotation_degrees: [0.0, 0.0, 90.0],
            translation_mm: [10.0, -3.0, 2.0],
            pivot: ModelPivot::BoundsCenter,
        };
        let b = bounds();
        let point = [2.0, 1.0, 0.0];
        let lhs = t.affine(&b).unwrap().transform_point(point).unwrap();
        let rhs = t.transform_point(point, &b).unwrap();
        for i in 0..3 {
            close(lhs[i], rhs[i]);
        }
        let transformed = t.transformed_bounds(&b).unwrap();
        assert!(transformed.dimensions().iter().all(|x| *x > 0.0));
        assert!(t.preview_matrix(&b, &transformed).unwrap().is_finite());
        t.rotation_degrees = [0.0; 3];
        t.scale_xyz = [1.0; 3];
        t.source_unit = StlSourceUnit::Millimeters;
        t.translation_mm = [0.0; 3];
        t.pivot = ModelPivot::StlOrigin;
        let g = t.preview_matrix(&b, &b).unwrap();
        assert_eq!(g, Mat4::IDENTITY);
    }

    #[test]
    fn invalid_scale_nonfinite_parameters_and_numeric_overflow_fail_closed() {
        let mut t = ModelTransform::default();
        t.scale_xyz[0] = 0.0;
        assert!(t.validate().is_err());
        t.scale_xyz[0] = -1.0;
        assert!(t.validate().is_err());
        t.scale_xyz[0] = 1.0;
        t.rotation_degrees[1] = f64::NAN;
        assert!(t.validate().is_err());
        t.rotation_degrees[1] = 0.0;
        t.translation_mm[2] = f64::INFINITY;
        assert!(t.validate().is_err());
        t.translation_mm[2] = 0.0;
        t.source_unit = StlSourceUnit::Inches;
        assert!(t.transform_point([f64::MAX, 1.0, 1.0], &bounds()).is_err());
    }
}

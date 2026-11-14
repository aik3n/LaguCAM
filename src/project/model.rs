use crate::stl::{Bounds, StlInfo};

#[derive(Clone, Debug)]
pub struct ModelReference {
    pub name: String,
    pub bounds: Bounds,
    pub triangles: u32,
    pub size_bytes: u64,
}

impl ModelReference {
    pub fn from_stl(name: String, info: StlInfo) -> Self {
        Self {
            name,
            bounds: info.bounds,
            triangles: info.triangles,
            size_bytes: info.size_bytes,
        }
    }

    /// Only for drawing. Returns normalized GPU coordinates; never use for CAM.
    pub fn visual_point(&self, point: [f64; 3]) -> Option<glam::Vec3> {
        if !point.iter().all(|v| v.is_finite()) {
            return None;
        }
        let dim = self.bounds.dimensions();
        let span = dim.into_iter().fold(0.0_f64, f64::max);
        if !span.is_finite() || span <= 0.0 {
            return None;
        }
        let center = self.bounds.center();
        let result = std::array::from_fn::<_, 3, _>(|axis| {
            ((point[axis] - center[axis]) / span * 2.0) as f32
        });
        if !result.iter().all(|v| v.is_finite()) {
            return None;
        }
        Some(glam::Vec3::from_array(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_transform_does_not_modify_real_bounds() {
        let bounds = Bounds {
            min: [1_000_000.0, -100.0, 0.0],
            max: [1_000_200.0, 100.0, 30.0],
        };
        let model = ModelReference {
            name: "test".into(),
            bounds,
            triangles: 2,
            size_bytes: 0,
        };
        assert_eq!(
            model.visual_point([1_000_100.0, 0.0, 15.0]).unwrap(),
            glam::Vec3::ZERO
        );
        assert_eq!(model.bounds.min, bounds.min);
        assert_eq!(model.bounds.max, bounds.max);
    }
}

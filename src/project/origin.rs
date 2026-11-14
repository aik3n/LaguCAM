//! Referencia del origen CNC: ancla en bruto o caja envolvente STL, offset y orientación.
use super::stock::Stock;
use crate::stl::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginReference {
    Stock,
    Model,
}

impl OriginReference {
    pub const ALL: [Self; 2] = [Self::Stock, Self::Model];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stock => "Bruto",
            Self::Model => "Modelo STL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginPoint {
    /// Bits 0, 1, 2 indican X+, Y+, Z+, respectivamente.
    Corner(u8),
    TopCenter,
    Center,
}

impl OriginPoint {
    pub const ALL: [Self; 10] = [
        Self::Corner(0),
        Self::Corner(1),
        Self::Corner(2),
        Self::Corner(3),
        Self::Corner(4),
        Self::Corner(5),
        Self::Corner(6),
        Self::Corner(7),
        Self::TopCenter,
        Self::Center,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Corner(0) => "X- Y- Z-",
            Self::Corner(1) => "X+ Y- Z-",
            Self::Corner(2) => "X- Y+ Z-",
            Self::Corner(3) => "X+ Y+ Z-",
            Self::Corner(4) => "X- Y- Z+",
            Self::Corner(5) => "X+ Y- Z+",
            Self::Corner(6) => "X- Y+ Z+",
            Self::Corner(7) => "X+ Y+ Z+",
            Self::Corner(_) => "Esquina incorrecta",
            Self::TopCenter => "Centro superior",
            Self::Center => "Centro",
        }
    }

    fn fractions(self) -> Result<[f64; 3], &'static str> {
        match self {
            Self::Corner(bits) if bits < 8 => Ok([
                f64::from(bits & 1),
                f64::from((bits >> 1) & 1),
                f64::from((bits >> 2) & 1),
            ]),
            Self::Corner(_) => Err("Esquina CNC no válida"),
            Self::TopCenter => Ok([0.5, 0.5, 1.0]),
            Self::Center => Ok([0.5; 3]),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkOrigin {
    pub reference: OriginReference,
    pub point: OriginPoint,
    /// Offset en coordenadas del modelo, independiente de la rotación CNC.
    pub offset: [f64; 3],
    /// Ángulos Euler en grados: R = Rz * Ry * Rx.
    pub rotation_degrees: [f64; 3],
    pub visible: bool,
}

impl Default for WorkOrigin {
    fn default() -> Self {
        Self {
            reference: OriginReference::Stock,
            point: OriginPoint::Corner(4),
            offset: [0.0; 3],
            rotation_degrees: [0.0; 3],
            visible: true,
        }
    }
}

impl WorkOrigin {
    /// Posición del ancla antes del offset. Se recalcula si cambia el bruto.
    pub fn anchor_position(
        &self,
        stock: &Stock,
        model_bounds: &Bounds,
        reference: OriginReference,
        point: OriginPoint,
    ) -> Result<[f64; 3], &'static str> {
        stock.validate()?;
        let (min, max) = match reference {
            OriginReference::Stock => (stock.min, stock.max().ok_or("Bruto no finito")?),
            OriginReference::Model => (model_bounds.min, model_bounds.max),
        };
        if !min.iter().chain(max.iter()).all(|v| v.is_finite()) {
            return Err("Límites de referencia no finitos");
        }
        let weights = point.fractions()?;
        let mut position = [0.0; 3];
        for (axis, value) in position.iter_mut().enumerate() {
            if max[axis] < min[axis] {
                return Err("Límites de referencia invertidos");
            }
            let span = max[axis] - min[axis];
            // Evita multiplicaciones inválidas (p. ej. 0 * infinito).
            if !span.is_finite() {
                return Err("Dimensiones de referencia fuera del rango f64");
            }
            *value = min[axis] + span * weights[axis];
        }
        if !position.iter().all(|v| v.is_finite()) {
            return Err("Ancla CNC fuera del rango f64");
        }
        Ok(position)
    }

    pub fn model_position(
        &self,
        stock: &Stock,
        model_bounds: &Bounds,
    ) -> Result<[f64; 3], &'static str> {
        if !self.offset.iter().all(|v| v.is_finite()) {
            return Err("Offset CNC no finito");
        }
        let anchor = self.anchor_position(stock, model_bounds, self.reference, self.point)?;
        let result = std::array::from_fn::<_, 3, _>(|i| anchor[i] + self.offset[i]);
        if !result.iter().all(|v| v.is_finite()) {
            return Err("Posición CNC fuera del rango f64");
        }
        Ok(result)
    }

    /// Matriz CNC -> modelo, representada como tres columnas X, Y, Z.
    pub fn rotation_basis(&self) -> Result<[[f64; 3]; 3], &'static str> {
        if !self.rotation_degrees.iter().all(|v| v.is_finite()) {
            return Err("Las rotaciones CNC deben ser finitas");
        }
        let (sx, cx) = self.rotation_degrees[0]
            .rem_euclid(360.0)
            .to_radians()
            .sin_cos();
        let (sy, cy) = self.rotation_degrees[1]
            .rem_euclid(360.0)
            .to_radians()
            .sin_cos();
        let (sz, cz) = self.rotation_degrees[2]
            .rem_euclid(360.0)
            .to_radians()
            .sin_cos();
        let basis = [
            [cy * cz, cy * sz, -sy],
            [sx * sy * cz - cx * sz, sx * sy * sz + cx * cz, sx * cy],
            [cx * sy * cz + sx * sz, cx * sy * sz - sx * cz, cx * cy],
        ];
        if !basis.iter().flatten().all(|v| v.is_finite()) {
            return Err("Orientación CNC fuera del rango f64");
        }
        Ok(basis)
    }

    pub fn work_to_model(
        &self,
        stock: &Stock,
        model_bounds: &Bounds,
        point_in_work: [f64; 3],
    ) -> Result<[f64; 3], &'static str> {
        if !point_in_work.iter().all(|v| v.is_finite()) {
            return Err("Punto CNC no finito");
        }
        let origin = self.model_position(stock, model_bounds)?;
        let basis = self.rotation_basis()?;
        let result = std::array::from_fn(|i| {
            origin[i]
                + basis[0][i] * point_in_work[0]
                + basis[1][i] * point_in_work[1]
                + basis[2][i] * point_in_work[2]
        });
        if !result.iter().all(|v| v.is_finite()) {
            return Err("Transformación CNC fuera del rango f64");
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub fn model_to_work(
        &self,
        stock: &Stock,
        model_bounds: &Bounds,
        point_in_model: [f64; 3],
    ) -> Result<[f64; 3], &'static str> {
        if !point_in_model.iter().all(|v| v.is_finite()) {
            return Err("Punto del modelo no finito");
        }
        let origin = self.model_position(stock, model_bounds)?;
        let basis = self.rotation_basis()?;
        let delta = std::array::from_fn::<_, 3, _>(|i| point_in_model[i] - origin[i]);
        let result = basis.map(|axis| axis[0] * delta[0] + axis[1] * delta[1] + axis[2] * delta[2]);
        if !result.iter().all(|v| v.is_finite()) {
            return Err("Transformación CNC fuera del rango f64");
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> Stock {
        Stock {
            min: [10.0, 20.0, 30.0],
            size: [100.0, 80.0, 20.0],
            visible: true,
        }
    }

    fn bounds() -> Bounds {
        Bounds {
            min: [15.0, 25.0, 35.0],
            max: [105.0, 95.0, 45.0],
        }
    }

    fn near(actual: [f64; 3], expected: [f64; 3]) {
        for (i, (value, target)) in actual.into_iter().zip(expected).enumerate() {
            assert!(
                (value - target).abs() < 1.0e-9,
                "axis {i}: {actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn stock_and_model_reference_are_independent() {
        let mut origin = WorkOrigin::default();
        assert_eq!(
            origin.model_position(&stock(), &bounds()).unwrap(),
            [10.0, 20.0, 50.0]
        );
        origin.reference = OriginReference::Model;
        assert_eq!(
            origin.model_position(&stock(), &bounds()).unwrap(),
            [15.0, 25.0, 45.0]
        );
        origin.point = OriginPoint::Corner(7);
        origin.offset = [2.0, -3.0, 5.0];
        assert_eq!(
            origin.model_position(&stock(), &bounds()).unwrap(),
            [107.0, 92.0, 50.0]
        );
        origin.reference = OriginReference::Stock;
        assert_eq!(
            origin.model_position(&stock(), &bounds()).unwrap(),
            [112.0, 97.0, 55.0]
        );
    }

    #[test]
    fn all_anchors_and_centers_are_valid() {
        let origin = WorkOrigin::default();
        for reference in OriginReference::ALL {
            for point in OriginPoint::ALL {
                assert!(
                    origin
                        .anchor_position(&stock(), &bounds(), reference, point)
                        .is_ok()
                );
            }
        }
        assert_eq!(
            origin
                .anchor_position(
                    &stock(),
                    &bounds(),
                    OriginReference::Stock,
                    OriginPoint::TopCenter
                )
                .unwrap(),
            [60.0, 60.0, 50.0]
        );
        assert_eq!(
            origin
                .anchor_position(
                    &stock(),
                    &bounds(),
                    OriginReference::Model,
                    OriginPoint::Center
                )
                .unwrap(),
            [60.0, 60.0, 40.0]
        );
    }

    #[test]
    fn offset_is_not_rotated_and_inverse_is_stable() {
        let origin = WorkOrigin {
            rotation_degrees: [25.0, -40.0, 70.0],
            offset: [1.0, -2.0, 3.0],
            ..Default::default()
        };
        near(
            origin.model_position(&stock(), &bounds()).unwrap(),
            [11.0, 18.0, 53.0],
        );
        let work = [7.5, -2.25, 3.75];
        let model = origin.work_to_model(&stock(), &bounds(), work).unwrap();
        near(
            origin.model_to_work(&stock(), &bounds(), model).unwrap(),
            work,
        );
    }

    #[test]
    fn rejects_non_finite_offset_rotation_and_invalid_corner() {
        let mut origin = WorkOrigin::default();
        origin.offset[1] = f64::NAN;
        assert!(origin.model_position(&stock(), &bounds()).is_err());
        origin.offset[1] = 0.0;
        origin.rotation_degrees[0] = f64::INFINITY;
        assert!(origin.rotation_basis().is_err());
        assert!(origin.work_to_model(&stock(), &bounds(), [0.0; 3]).is_err());
        assert!(
            origin
                .anchor_position(
                    &stock(),
                    &bounds(),
                    OriginReference::Stock,
                    OriginPoint::Corner(8)
                )
                .is_err()
        );
    }
}

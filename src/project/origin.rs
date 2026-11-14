use super::stock::Stock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginPreset {
    LowerMin,
    UpperMin,
    UpperCenter,
    Center,
    Custom,
}

impl OriginPreset {
    pub const ALL: [Self; 5] = [
        Self::LowerMin,
        Self::UpperMin,
        Self::UpperCenter,
        Self::Center,
        Self::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::LowerMin => "Esquina inferior mínima",
            Self::UpperMin => "Esquina superior mínima",
            Self::UpperCenter => "Centro de la cara superior",
            Self::Center => "Centro del bruto",
            Self::Custom => "Coordenadas personalizadas",
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkOrigin {
    pub preset: OriginPreset,
    /// Relative to stock minimum, in real model units.
    pub custom_offset: [f64; 3],
    /// Angulos de Euler XYZ extrinsecos en grados; R = Rz * Ry * Rx.
    pub rotation_degrees: [f64; 3],
    pub visible: bool,
}

impl Default for WorkOrigin {
    fn default() -> Self {
        Self {
            preset: OriginPreset::UpperMin,
            custom_offset: [0.0; 3],
            rotation_degrees: [0.0; 3],
            visible: true,
        }
    }
}

impl WorkOrigin {
    pub fn offset(&self, stock: &Stock) -> [f64; 3] {
        let [x, y, z] = stock.size;
        match self.preset {
            OriginPreset::LowerMin => [0.0, 0.0, 0.0],
            OriginPreset::UpperMin => [0.0, 0.0, z],
            OriginPreset::UpperCenter => [x / 2.0, y / 2.0, z],
            OriginPreset::Center => [x / 2.0, y / 2.0, z / 2.0],
            OriginPreset::Custom => self.custom_offset,
        }
    }
    pub fn model_position(&self, stock: &Stock) -> Result<[f64; 3], &'static str> {
        stock.validate()?;
        if !self.custom_offset.iter().all(|v| v.is_finite()) {
            return Err("Desplazamiento CNC no finito");
        }
        let offset = self.offset(stock);
        let position = std::array::from_fn::<_, 3, _>(|i| stock.min[i] + offset[i]);
        if position.iter().all(|v| v.is_finite()) {
            Ok(position)
        } else {
            Err("La posición CNC excede el rango f64")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_and_custom_offset() {
        let stock = Stock {
            min: [10.0, 20.0, 30.0],
            size: [100.0, 80.0, 20.0],
            visible: true,
        };
        let mut origin = WorkOrigin::default();
        assert_eq!(origin.model_position(&stock).unwrap(), [10.0, 20.0, 50.0]);
        origin.preset = OriginPreset::UpperCenter;
        assert_eq!(origin.model_position(&stock).unwrap(), [60.0, 60.0, 50.0]);
        origin.preset = OriginPreset::Custom;
        origin.custom_offset = [1.0, -2.0, 3.0];
        assert_eq!(origin.model_position(&stock).unwrap(), [11.0, 18.0, 33.0]);
        origin.custom_offset[2] = f64::NAN;
        assert!(origin.model_position(&stock).is_err());
    }
}

// LaguCAM 004: orientación del sistema de trabajo sin rotar modelo ni bruto.
impl WorkOrigin {
    /// Matriz de rotación local CNC -> modelo como TRES COLUMNAS (ejes X,Y,Z).
    /// Angulos en grados. R = Rz * Ry * Rx (rotaciones activas extrínsecas
    /// sobre los ejes fijos del modelo; sistema de mano derecha).
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

    /// Transforma un punto expresado en CNC hacia las coordenadas originales STL.
    pub fn work_to_model(
        &self,
        stock: &Stock,
        point_in_work: [f64; 3],
    ) -> Result<[f64; 3], &'static str> {
        if !point_in_work.iter().all(|v| v.is_finite()) {
            return Err("Punto CNC no finito");
        }
        let origin = self.model_position(stock)?;
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

    /// Inversa del sistema rígido CNC (R traspuesta). Reserva para futuro CAM.
    #[allow(dead_code)]
    pub fn model_to_work(
        &self,
        stock: &Stock,
        point_in_model: [f64; 3],
    ) -> Result<[f64; 3], &'static str> {
        if !point_in_model.iter().all(|v| v.is_finite()) {
            return Err("Punto del modelo no finito");
        }
        let origin = self.model_position(stock)?;
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
mod rotation_tests {
    use super::*;

    fn stock() -> Stock {
        Stock {
            min: [10.0, 20.0, 30.0],
            size: [100.0, 80.0, 20.0],
            visible: true,
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
    fn zero_rotation_preserves_work_axes_and_presets() {
        let origin = WorkOrigin::default();
        near(
            origin.work_to_model(&stock(), [2.0, 3.0, 4.0]).unwrap(),
            [12.0, 23.0, 54.0],
        );
        near(
            origin.model_to_work(&stock(), [12.0, 23.0, 54.0]).unwrap(),
            [2.0, 3.0, 4.0],
        );
    }

    #[test]
    fn each_axis_rotates_independently() {
        let mut origin = WorkOrigin {
            rotation_degrees: [90.0, 0.0, 0.0],
            ..Default::default()
        };
        near(
            origin.work_to_model(&stock(), [0.0, 1.0, 0.0]).unwrap(),
            [10.0, 20.0, 51.0],
        );
        origin.rotation_degrees = [0.0, 90.0, 0.0];
        near(
            origin.work_to_model(&stock(), [1.0, 0.0, 0.0]).unwrap(),
            [10.0, 20.0, 49.0],
        );
        origin.rotation_degrees = [0.0, 0.0, 90.0];
        near(
            origin.work_to_model(&stock(), [1.0, 0.0, 0.0]).unwrap(),
            [10.0, 21.0, 50.0],
        );
    }

    #[test]
    fn combined_rotation_is_invertible_without_changing_stock() {
        let stock = stock();
        let origin = WorkOrigin {
            rotation_degrees: [25.0, -40.0, 70.0],
            ..Default::default()
        };
        let work = [7.5, -2.25, 3.75];
        let model = origin.work_to_model(&stock, work).unwrap();
        near(origin.model_to_work(&stock, model).unwrap(), work);
        assert_eq!(stock.min, [10.0, 20.0, 30.0]);
        assert_eq!(stock.size, [100.0, 80.0, 20.0]);
    }

    #[test]
    fn rejects_invalid_rotation_and_coordinates() {
        let mut origin = WorkOrigin::default();
        origin.rotation_degrees[1] = f64::INFINITY;
        assert!(origin.rotation_basis().is_err());
        assert!(origin.work_to_model(&stock(), [0.0; 3]).is_err());
        origin.rotation_degrees[1] = f64::NAN;
        assert!(origin.rotation_basis().is_err());
        origin.rotation_degrees[1] = 0.0;
        assert!(
            origin
                .work_to_model(&stock(), [f64::NAN, 0.0, 0.0])
                .is_err()
        );
    }
}

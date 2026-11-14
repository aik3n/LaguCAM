use crate::stl::Bounds;

#[derive(Clone, Debug, PartialEq)]
pub struct Stock {
    /// Lower corner in original STL/model coordinates.
    pub min: [f64; 3],
    pub size: [f64; 3],
    pub visible: bool,
}

impl Stock {
    pub fn from_bounds(bounds: &Bounds) -> Self {
        let dims = bounds.dimensions();
        Self {
            min: bounds.min,
            size: dims.map(|d| if d > 0.0 && d.is_finite() { d } else { 1.0 }),
            visible: true,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.min.iter().all(|v| v.is_finite()) {
            return Err("Posición del bruto no finita");
        }
        if !self.size.iter().all(|v| v.is_finite() && *v > 0.0) {
            return Err("Las dimensiones del bruto deben ser positivas y finitas");
        }
        if self.max().is_none() {
            return Err("Los límites del bruto exceden el rango de f64");
        }
        Ok(())
    }

    pub fn max(&self) -> Option<[f64; 3]> {
        let max = std::array::from_fn::<_, 3, _>(|i| self.min[i] + self.size[i]);
        max.iter().all(|v| v.is_finite()).then_some(max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_stock_uses_real_bounds() {
        let b = Bounds {
            min: [-12.5, 4.0, -2.0],
            max: [8.5, 14.0, 6.0],
        };
        let stock = Stock::from_bounds(&b);
        assert_eq!(stock.min, b.min);
        assert_eq!(stock.size, [21.0, 10.0, 8.0]);
        assert!(stock.validate().is_ok());
    }
    #[test]
    fn rejects_invalid_values() {
        let mut s = Stock {
            min: [0.0; 3],
            size: [1.0; 3],
            visible: true,
        };
        s.size[0] = 0.0;
        assert!(s.validate().is_err());
        s.size[0] = -1.0;
        assert!(s.validate().is_err());
        s.size[0] = f64::NAN;
        assert!(s.validate().is_err());
        s.size[0] = 1.0;
        s.min[1] = f64::INFINITY;
        assert!(s.validate().is_err());
        s.min[1] = f64::MAX;
        s.size[1] = f64::MAX;
        assert!(s.validate().is_err());
    }
}

// LaguCAM 004: holguras independientes por las seis caras del modelo STL.
impl Stock {
    /// Orden: X-, X+, Y-, Y+, Z-, Z+. Valores en unidades originales del STL.
    /// La caja sigue alineada con los ejes del modelo y sus datos siguen en f64.
    pub fn from_extras(
        bounds: &Bounds,
        extras: [f64; 6],
        visible: bool,
    ) -> Result<Self, &'static str> {
        if !extras.iter().all(|v| v.is_finite() && *v >= 0.0) {
            return Err("Los extras deben ser finitos y no negativos");
        }
        if !bounds
            .min
            .iter()
            .chain(bounds.max.iter())
            .all(|v| v.is_finite())
        {
            return Err("Los límites STL no son finitos");
        }
        let min = std::array::from_fn(|i| bounds.min[i] - extras[2 * i]);
        let max: [f64; 3] = std::array::from_fn(|i| bounds.max[i] + extras[2 * i + 1]);
        let size = std::array::from_fn(|i| max[i] - min[i]);
        let stock = Self { min, size, visible };
        stock.validate()?;
        Ok(stock)
    }

    /// Holguras con signo respecto al STL. Un valor negativo indica que el
    /// bruto editado manualmente no contiene por completo el modelo en ese lado.
    pub fn extras_from_bounds(&self, bounds: &Bounds) -> Result<[f64; 6], &'static str> {
        self.validate()?;
        let max = self.max().ok_or("Límites del bruto no finitos")?;
        let result = [
            bounds.min[0] - self.min[0],
            max[0] - bounds.max[0],
            bounds.min[1] - self.min[1],
            max[1] - bounds.max[1],
            bounds.min[2] - self.min[2],
            max[2] - bounds.max[2],
        ];
        if !result.iter().all(|v| v.is_finite()) {
            return Err("Holguras no finitas");
        }
        Ok(result)
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn independent_extras_define_min_and_max() {
        let bounds = Bounds {
            min: [10.0, 20.0, 30.0],
            max: [110.0, 100.0, 60.0],
        };
        let stock = Stock::from_extras(&bounds, [2.0, 4.0, 1.0, 3.0, 0.0, 10.0], true).unwrap();
        assert_eq!(stock.min, [8.0, 19.0, 30.0]);
        assert_eq!(stock.size, [106.0, 84.0, 40.0]);
        assert_eq!(
            stock.extras_from_bounds(&bounds).unwrap(),
            [2.0, 4.0, 1.0, 3.0, 0.0, 10.0]
        );
    }

    #[test]
    fn rejects_negative_nan_infinite_and_overflowing_extras() {
        let bounds = Bounds {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        for invalid in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut extras = [0.0; 6];
            extras[3] = invalid;
            assert!(Stock::from_extras(&bounds, extras, true).is_err());
        }
        let overflow_bounds = Bounds {
            min: [-f64::MAX, 0.0, 0.0],
            max: [f64::MAX, 1.0, 1.0],
        };
        assert!(Stock::from_extras(&overflow_bounds, [0.0; 6], true).is_err());
    }

    #[test]
    fn zero_thickness_requires_an_extra_on_at_least_one_side() {
        let bounds = Bounds {
            min: [0.0; 3],
            max: [5.0, 5.0, 0.0],
        };
        assert!(Stock::from_extras(&bounds, [0.0; 6], true).is_err());
        let stock = Stock::from_extras(&bounds, [0.0, 0.0, 0.0, 0.0, 0.0, 1.0], true).unwrap();
        assert_eq!(stock.size[2], 1.0);
    }

    #[test]
    fn manual_box_reports_signed_extras() {
        let bounds = Bounds {
            min: [0.0; 3],
            max: [10.0; 3],
        };
        let stock = Stock {
            min: [1.0, 0.0, 0.0],
            size: [9.0, 10.0, 10.0],
            visible: false,
        };
        assert_eq!(stock.extras_from_bounds(&bounds).unwrap()[0], -1.0);
    }
}

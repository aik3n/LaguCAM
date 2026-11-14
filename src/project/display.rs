//! Propiedades visuales del bruto, independientes de la geometría CAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockMode {
    Solid,
    Edges,
    SolidEdges,
}

impl StockMode {
    pub const ALL: [Self; 3] = [Self::SolidEdges, Self::Solid, Self::Edges];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Sólido translúcido",
            Self::Edges => "Solo aristas",
            Self::SolidEdges => "Sólido y aristas",
        }
    }

    pub fn has_faces(self) -> bool {
        matches!(self, Self::Solid | Self::SolidEdges)
    }

    pub fn has_edges(self) -> bool {
        matches!(self, Self::Edges | Self::SolidEdges)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockColor {
    Amber,
    Blue,
    Gray,
    Green,
}

impl StockColor {
    pub const ALL: [Self; 4] = [Self::Amber, Self::Blue, Self::Gray, Self::Green];

    pub fn label(self) -> &'static str {
        match self {
            Self::Amber => "Ámbar",
            Self::Blue => "Azul",
            Self::Gray => "Gris",
            Self::Green => "Verde",
        }
    }

    pub fn rgb(self) -> [f32; 3] {
        match self {
            Self::Amber => [0.85, 0.62, 0.28],
            Self::Blue => [0.30, 0.58, 0.83],
            Self::Gray => [0.61, 0.65, 0.69],
            Self::Green => [0.37, 0.68, 0.52],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StockDisplay {
    pub mode: StockMode,
    pub color: StockColor,
    /// Opacidad del sólido en porcentaje [0, 100]. Las aristas son opacas.
    pub opacity_percent: u8,
}

impl Default for StockDisplay {
    fn default() -> Self {
        Self {
            mode: StockMode::SolidEdges,
            color: StockColor::Amber,
            opacity_percent: 25,
        }
    }
}

impl StockDisplay {
    pub fn opacity(self) -> f32 {
        f32::from(self.opacity_percent.min(100)) / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_modes() {
        let display = StockDisplay::default();
        assert_eq!(display.opacity(), 0.25);
        assert!(display.mode.has_faces());
        assert!(display.mode.has_edges());
        assert!(!StockMode::Edges.has_faces());
        assert!(!StockMode::Solid.has_edges());
    }

    #[test]
    fn alpha_bounds() {
        let mut d = StockDisplay {
            opacity_percent: 0,
            ..Default::default()
        };
        assert_eq!(d.opacity(), 0.0);
        d.opacity_percent = 100;
        assert_eq!(d.opacity(), 1.0);
        d.opacity_percent = u8::MAX;
        assert_eq!(d.opacity(), 1.0);
        for color in StockColor::ALL {
            assert!(color.rgb().iter().all(|v| (0.0..=1.0).contains(v)));
        }
    }
}

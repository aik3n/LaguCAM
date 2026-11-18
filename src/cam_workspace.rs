//! UI-CAM-01: navegacion UI. No es un motor CAM, NC ni un estado persistido.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Operation,
    Gcode,
    Simulation,
    Verification,
}

impl Tab {
    pub const ALL: [Self; 4] = [
        Self::Operation,
        Self::Gcode,
        Self::Simulation,
        Self::Verification,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Operation => "Operación",
            Self::Gcode => "G-code",
            Self::Simulation => "Simulación",
            Self::Verification => "Verificación",
        }
    }
}

pub struct CamWorkspace {
    pub open: bool,
    pub tab: Tab,
}

impl Default for CamWorkspace {
    fn default() -> Self {
        Self {
            open: true,
            tab: Tab::Operation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_tabs_are_unique_and_ordered() {
        assert_eq!(Tab::ALL.len(), 4);
        assert_eq!(Tab::ALL[0], Tab::Operation);
        assert_eq!(Tab::ALL[3], Tab::Verification);
    }
}

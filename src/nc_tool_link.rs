//! Correspondencia de fresa entre operacion CAM y programa NC interpretado.
//! Solo la procedencia conservada al GENERAR el NC demuestra la identidad de fresa.
//! T/M6 no dispone de equivalencia automatica con UUID de biblioteca.
use crate::tools::{cam_adapter::ToolSignature, hash::geometry_sha256, model::Tool};
use cam_libre_visor::nc_contract::{NcEventKind, NcTrace};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolLinkStatus {
    Verified,
    Manual,
    Unproven,
}

impl ToolLinkStatus {
    pub fn color(self) -> egui::Color32 {
        match self {
            Self::Verified => egui::Color32::LIGHT_GREEN,
            Self::Manual => egui::Color32::YELLOW,
            Self::Unproven => egui::Color32::LIGHT_RED,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => {
                "Verificada: mismo UUID y geometria que CAM (no certifica seguridad CNC)"
            }
            Self::Manual => "Seleccionada manualmente; NC sin procedencia de fresa demostrable",
            Self::Unproven => {
                "Sin correspondencia demostrada; revisa fresa, geometria y comandos T/M6"
            }
        }
    }
}

/// Cualquier T/M6 en la traza rompe la hipotesis de una unica fresa CAM,
/// incluso si el numero T coincide visualmente con un identificador del operador.
pub fn trace_changes_tool(trace: &NcTrace) -> bool {
    trace.events.iter().any(|event| {
        matches!(
            event.kind,
            NcEventKind::SelectTool(_) | NcEventKind::ToolChange(_)
        )
    })
}

/// No se equiparan fresas por nombre, diametro parcial ni numero T.
/// Si el NC fue importado, la eleccion manual no equivale a prueba de identidad.
pub fn evaluate_tool_link(
    expected: Option<&ToolSignature>,
    selected: Option<&Tool>,
    tool_commands_present: bool,
) -> ToolLinkStatus {
    let Some(tool) = selected else {
        return ToolLinkStatus::Unproven;
    };
    if tool.validate().is_err() {
        return ToolLinkStatus::Unproven;
    }
    let Some(expected) = expected else {
        return ToolLinkStatus::Manual;
    };
    if tool_commands_present || expected.tool_id != tool.id {
        return ToolLinkStatus::Unproven;
    }
    if geometry_sha256(tool).ok() == Some(expected.geometry_hash) {
        ToolLinkStatus::Verified
    } else {
        ToolLinkStatus::Unproven
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::model::{ToolGeometry, ToolKind};

    fn cutter() -> Tool {
        let mut tool = Tool::blank(ToolKind::Flat);
        tool.id = "9e20f75a-a941-4b2d-a0e5-010000000001".into();
        tool.revision = 1;
        tool.name = "Plana 4 mm".into();
        tool.geometry = ToolGeometry::Flat {
            cutting_diameter_mm: 4.0,
        };
        tool.cutting_length_mm = 15.0;
        tool.shank_diameter_mm = 4.0;
        tool.total_length_mm = 30.0;
        tool
    }

    #[test]
    fn generated_matches_only_uuid_and_full_geometry() {
        let tool = cutter();
        let expected = ToolSignature {
            tool_id: tool.id.clone(),
            geometry_hash: geometry_sha256(&tool).unwrap(),
        };
        assert_eq!(
            evaluate_tool_link(Some(&expected), Some(&tool), false),
            ToolLinkStatus::Verified
        );
        let mut renamed = tool.clone();
        renamed.name = "Mismo perfil, distinto nombre".into();
        assert_eq!(
            evaluate_tool_link(Some(&expected), Some(&renamed), false),
            ToolLinkStatus::Verified
        );
        let mut changed = tool.clone();
        changed.geometry = ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        };
        assert_eq!(
            evaluate_tool_link(Some(&expected), Some(&changed), false),
            ToolLinkStatus::Unproven
        );
        let mut another_id = tool.clone();
        another_id.id = "9e20f75a-a941-4b2d-a0e5-010000000002".into();
        assert_eq!(
            evaluate_tool_link(Some(&expected), Some(&another_id), false),
            ToolLinkStatus::Unproven
        );
        assert_eq!(
            evaluate_tool_link(Some(&expected), None, false),
            ToolLinkStatus::Unproven
        );
    }

    #[test]
    fn imported_nc_is_always_manual_or_unproven() {
        assert_eq!(
            evaluate_tool_link(None, Some(&cutter()), false),
            ToolLinkStatus::Manual
        );
        assert_eq!(
            evaluate_tool_link(None, None, false),
            ToolLinkStatus::Unproven
        );
    }

    #[test]
    fn tool_change_cannot_be_equated_to_library_uuid() {
        let tool = cutter();
        let expected = ToolSignature {
            tool_id: tool.id.clone(),
            geometry_hash: geometry_sha256(&tool).unwrap(),
        };
        assert_eq!(
            evaluate_tool_link(Some(&expected), Some(&tool), true),
            ToolLinkStatus::Unproven
        );
    }

    #[test]
    fn nc_tool_event_is_detected_without_number_to_uuid_assumption() {
        use cam_libre_visor::nc_contract::{
            NcDistanceMode, NcEvent, NcInitialState, NcSpindleMode, NcUnits,
        };
        let trace = NcTrace {
            initial: NcInitialState {
                position_work_mm: [0.0; 3],
                units: NcUnits::Millimeters,
                distance_mode: NcDistanceMode::Absolute,
                feed_mm_min: None,
                spindle_rpm: None,
                spindle_mode: NcSpindleMode::Stopped,
                tool_number: None,
            },
            events: vec![NcEvent {
                source_line: 5,
                kind: NcEventKind::SelectTool(2),
            }],
        };
        assert!(trace_changes_tool(&trace));
    }
}

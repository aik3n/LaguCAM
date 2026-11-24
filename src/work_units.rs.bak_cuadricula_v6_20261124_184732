//! Unidades de presentacion y entrada: el dominio CAM permanece en mm.
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkUnits {
    #[default]
    Mm,
    Inch,
}

impl WorkUnits {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Inch => "in",
        }
    }

    pub fn mm_per_unit(self) -> f64 {
        match self {
            Self::Mm => 1.0,
            Self::Inch => 25.4,
        }
    }

    pub fn display(self, mm: f64) -> f64 {
        mm / self.mm_per_unit()
    }

    pub fn length(self, mm: f64, precision: usize) -> String {
        format!("{:.*} {}", precision, self.display(mm), self.symbol())
    }

    pub fn feed(self, mm_per_min: f64, precision: usize) -> String {
        format!(
            "{:.*} {}/min",
            precision,
            self.display(mm_per_min),
            self.symbol()
        )
    }

    /// Editor de una longitud o avance almacenados en mm. El cambio de unidad
    /// no altera el dato hasta que el usuario modifica efectivamente el valor.
    pub fn drag_mm(
        self,
        ui: &mut egui::Ui,
        mm: &mut f64,
        speed_mm: f64,
        range_mm: Option<(f64, f64)>,
    ) -> egui::Response {
        let factor = self.mm_per_unit();
        let mut view = *mm / factor;
        let mut drag = egui::DragValue::new(&mut view)
            .speed(speed_mm / factor)
            .max_decimals(8);
        if let Some((min, max)) = range_mm {
            drag = drag.range((min / factor)..=(max / factor));
        }
        let response = ui.add(drag);
        if response.changed() {
            let physical = view * factor;
            if physical.is_finite() {
                *mm = physical;
            }
        }
        response
    }
}

const SETTINGS_ID: &str = "lagucam_work_units_display_v1";

pub fn publish(ctx: &egui::Context, units: WorkUnits) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(SETTINGS_ID), units));
    ctx.request_repaint();
}

pub fn current(ctx: &egui::Context) -> WorkUnits {
    ctx.data(|data| data.get_temp::<WorkUnits>(egui::Id::unique(SETTINGS_ID)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imperial_and_metric_length_and_feed() {
        assert_eq!(WorkUnits::Mm.display(25.4), 25.4);
        assert!((WorkUnits::Inch.display(25.4) - 1.0).abs() < 1.0e-12);
        assert!((WorkUnits::Inch.display(254.0) - 10.0).abs() < 1.0e-12);
        assert_eq!(WorkUnits::Inch.length(25.4, 3), "1.000 in");
        assert_eq!(WorkUnits::Inch.feed(254.0, 2), "10.00 in/min");
    }

    #[test]
    fn settings_roundtrip_and_default() {
        for units in [WorkUnits::Mm, WorkUnits::Inch] {
            let json = serde_json::to_string(&units).unwrap();
            assert_eq!(serde_json::from_str::<WorkUnits>(&json).unwrap(), units);
        }
        assert_eq!(WorkUnits::default(), WorkUnits::Mm);
    }

    #[test]
    fn context_remembers_work_units_without_mutating_model() {
        let ctx = egui::Context::default();
        assert_eq!(current(&ctx), WorkUnits::Mm);
        publish(&ctx, WorkUnits::Inch);
        assert_eq!(current(&ctx), WorkUnits::Inch);
        publish(&ctx, WorkUnits::Mm);
        assert_eq!(current(&ctx), WorkUnits::Mm);
    }
}

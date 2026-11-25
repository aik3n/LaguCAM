//! Integracion entre la traza NC interpretada y el motor de material disperso.
//!
//! Es un servicio de dominio de LaguCAM, sin ventanas ni renderizadores.
//! Mantiene el mismo contrato de coordenadas que material_sim::segments_from_trace.
//! La visualizacion seguira usando el motor anterior hasta conectar un render
//! que represente todas las muestras, sin decimacion silenciosa.

use crate::{
    material_sim::{CutSegment, segments_from_trace},
    nc_contract::NcTrace,
    sparse_material::{SparseConfig, SparseMaterial, SparseStats},
};

pub struct SparseNcPlayback {
    segments: Vec<CutSegment>,
    material: SparseMaterial,
}

impl SparseNcPlayback {
    /// Construye una sesion desde el NC ya interpretado (no desde trayectorias CAM).
    /// `to_model` es la misma transformacion trabajo -> modelo que usa la UI actual.
    pub fn from_trace(
        trace: &NcTrace,
        config: SparseConfig,
        to_model: impl FnMut([f64; 3]) -> Result<[f64; 3], String>,
    ) -> Result<Self, String> {
        let segments = segments_from_trace(trace, to_model)?;
        let material = SparseMaterial::new(config)?;
        Ok(Self { segments, material })
    }

    /// Posicion de la reproduccion: numero de movimientos completos + fraccion.
    pub fn seek(&mut self, playhead: f64) -> Result<(), String> {
        self.material.seek(&self.segments, playhead)
    }

    pub fn move_count(&self) -> usize {
        self.segments.len()
    }

    pub fn cursor(&self) -> f64 {
        self.material.stats().cursor
    }

    pub fn stats(&self) -> SparseStats {
        self.material.stats()
    }

    pub fn cells(&self) -> [usize; 2] {
        self.material.cells()
    }

    pub fn spacing_mm(&self) -> [f64; 2] {
        self.material.spacing_mm()
    }

    pub fn height_at(&self, ix: usize, iy: usize) -> Option<f64> {
        self.material.height_at(ix, iy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        material_sim::{CutterShape, CutterSpec, StockBox},
        nc_contract::{NcDistanceMode, NcInitialState, NcSpindleMode, NcUnits},
        nc_interpreter::interpret_program,
    };

    fn initial() -> NcInitialState {
        NcInitialState {
            position_work_mm: [2.0, 5.0, 10.0],
            units: NcUnits::Millimeters,
            distance_mode: NcDistanceMode::Absolute,
            feed_mm_min: None,
            spindle_rpm: None,
            spindle_mode: NcSpindleMode::Stopped,
            tool_number: None,
        }
    }

    fn config() -> SparseConfig {
        SparseConfig {
            stock: StockBox {
                min_mm: [0.0, 0.0, 0.0],
                size_mm: [20.0, 20.0, 8.0],
            },
            cutter: CutterSpec {
                shape: CutterShape::Flat,
                diameter_mm: 2.0,
                cutting_length_mm: 5.0,
            },
            spacing_mm: 0.1,
            checkpoint_every: 2,
            max_checkpoints: 4,
            height_budget_mib: 16,
        }
    }

    #[test]
    fn plays_interpreted_nc_and_rewinds() {
        let trace = interpret_program("M3 S12000\nG1 X18 Y5 Z4 F200\nM30", initial()).unwrap();
        let mut playback = SparseNcPlayback::from_trace(&trace, config(), Ok).unwrap();
        assert_eq!(playback.move_count(), 1);
        assert_eq!(playback.cells(), [201, 201]);
        assert!((playback.spacing_mm()[0] - 0.1).abs() < 1e-12);
        playback.seek(1.0).unwrap();
        assert!(playback.height_at(180, 50).unwrap() < 8.0);
        let final_height = playback.height_at(180, 50).unwrap();
        playback.seek(0.0).unwrap();
        assert_eq!(playback.height_at(180, 50), Some(8.0));
        playback.seek(1.0).unwrap();
        assert_eq!(playback.height_at(180, 50), Some(final_height));
    }

    #[test]
    fn uses_work_to_model_transform_from_caller() {
        let trace = interpret_program("M3 S12000\nG1 X13 Y5 Z4 F200\nM30", initial()).unwrap();
        // Desplazamiento horizontal de +5 mm en el modelo.
        let mut playback = SparseNcPlayback::from_trace(&trace, config(), |work| {
            Ok([work[0] + 5.0, work[1], work[2]])
        })
        .unwrap();
        playback.seek(1.0).unwrap();
        assert!(playback.height_at(180, 50).unwrap() < 8.0);
    }

    #[test]
    fn preserves_rejection_of_multi_tool_trace() {
        let trace =
            interpret_program("T1 M6\nM3 S12000\nG1 X18 Y5 Z4 F200\nM30", initial()).unwrap();
        assert!(
            SparseNcPlayback::from_trace(&trace, config(), Ok)
                .err()
                .unwrap()
                .contains("herramienta")
        );
    }

    #[test]
    fn rejects_invalid_cursor_without_progress() {
        let trace = interpret_program("M3 S12000\nG1 X18 Y5 Z4 F200\nM30", initial()).unwrap();
        let mut playback = SparseNcPlayback::from_trace(&trace, config(), Ok).unwrap();
        assert!(playback.seek(2.0).is_err());
        assert_eq!(playback.cursor(), 0.0);
    }
}

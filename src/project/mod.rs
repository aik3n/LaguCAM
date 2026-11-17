pub mod cam_coordinates;
pub mod cam_provenance;
pub mod display;
pub mod model;
#[allow(dead_code)] // Se conecta a visor/CAM en P1-M2.
pub mod model_transform;
pub mod operations;
pub mod origin;
pub mod persistence;
pub mod selection;
pub mod stock;

use crate::stl::StlInfo;
use display::StockDisplay;
use model::ModelReference;
use model_transform::ModelTransform;
use operations::Operations;
use origin::WorkOrigin;
use selection::Selection;
use stock::Stock;

pub struct Project {
    pub model: Option<ModelReference>,
    pub model_transform: ModelTransform,
    pub stock: Option<Stock>,
    pub stock_display: StockDisplay,
    pub model_color: [u8; 3],
    pub origin: WorkOrigin,
    pub operations: Operations,
    pub selection: Selection,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            model: None,
            model_transform: ModelTransform::default(),
            stock: None,
            stock_display: StockDisplay::default(),
            model_color: [160, 180, 150],
            origin: WorkOrigin::default(),
            operations: Operations::default(),
            selection: Selection::default(),
        }
    }
}

impl Project {
    /// Aplica las unidades y transformaciones sin sobrescribir el STL original.
    /// El bruto vuelve a los limites fisicos del modelo; revisar ajustes manuales.
    pub fn set_model_transform(&mut self, value: ModelTransform) -> Result<(), String> {
        let model = self.model.as_ref().ok_or("No hay modelo STL abierto")?;
        let bounds = value.transformed_bounds(&model.raw_bounds)?;
        value.preview_matrix(&model.raw_bounds, &bounds)?;
        let stock = Stock::from_bounds(&bounds);
        stock.validate().map_err(str::to_owned)?;
        self.origin
            .model_position(&stock, &bounds)
            .map_err(str::to_owned)?;
        self.model_transform = value;
        self.model.as_mut().expect("Modelo verificado").bounds = bounds;
        self.stock = Some(stock);
        for operation in &mut self.operations.items {
            if let operations::OperationKind::ParallelTest(params) = &mut operation.kind {
                // La region XY anterior no identifica la misma zona de la pieza.
                params.area_xy = None;
                operation.status = operations::OperationStatus::NotCalculated;
            }
        }
        Ok(())
    }

    pub fn loaded(&mut self, name: String, info: StlInfo) {
        let model = ModelReference::from_stl(name, info);
        self.stock = Some(Stock::from_bounds(&model.bounds));
        self.stock_display = StockDisplay::default();
        self.model = Some(model);
        self.model_transform = ModelTransform::default();
        self.origin = WorkOrigin::default();
        self.selection = Selection::Model;
        self.operations.items.clear();
    }
}

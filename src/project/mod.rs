pub mod display;
pub mod model;
pub mod operations;
pub mod origin;
pub mod selection;
pub mod stock;

use crate::stl::StlInfo;
use display::StockDisplay;
use model::ModelReference;
use operations::Operations;
use origin::WorkOrigin;
use selection::Selection;
use stock::Stock;

pub struct Project {
    pub model: Option<ModelReference>,
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
    pub fn loaded(&mut self, name: String, info: StlInfo) {
        let model = ModelReference::from_stl(name, info);
        self.stock = Some(Stock::from_bounds(&model.bounds));
        self.stock_display = StockDisplay::default();
        self.model = Some(model);
        self.origin = WorkOrigin::default();
        self.selection = Selection::Model;
        self.operations.items.clear();
    }
}

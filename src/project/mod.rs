pub mod model;
pub mod operations;
pub mod origin;
pub mod selection;
pub mod stock;

use crate::stl::StlInfo;
use model::ModelReference;
use operations::Operations;
use origin::WorkOrigin;
use selection::Selection;
use stock::Stock;

#[derive(Default)]
pub struct Project {
    pub model: Option<ModelReference>,
    pub stock: Option<Stock>,
    pub origin: WorkOrigin,
    pub operations: Operations,
    pub selection: Selection,
}

impl Project {
    pub fn loaded(&mut self, name: String, info: StlInfo) {
        let model = ModelReference::from_stl(name, info);
        self.stock = Some(Stock::from_bounds(&model.bounds));
        self.model = Some(model);
        self.origin = WorkOrigin::default();
        self.selection = Selection::Model;
        self.operations.items.clear();
    }
}

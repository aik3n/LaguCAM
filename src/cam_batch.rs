//! UX-01E.1: estado de la cola de calculo CAM conjunto.
//! No ejecuta geometria, no genera NC y no persiste resultados en el proyecto.
use crate::project::{Project, operations::remap_index_after_move};
use std::collections::VecDeque;

pub struct BatchCalculation {
    pub queued: VecDeque<usize>,
    pub current: Option<usize>,
    pub total: usize,
    pub calculated: usize,
    pub reused: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl BatchCalculation {
    pub fn from_project(project: &Project) -> Option<Self> {
        let queued: VecDeque<usize> = project
            .operations
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, operation)| operation.active.then_some(index))
            .collect();
        if queued.is_empty() {
            return None;
        }
        Some(Self {
            total: queued.len(),
            queued,
            current: None,
            calculated: 0,
            reused: 0,
            skipped: 0,
            errors: Vec::new(),
        })
    }

    pub fn completed(&self) -> usize {
        self.calculated + self.reused + self.skipped + self.errors.len()
    }

    /// Mantener la identidad logica de cada trabajo si el arbol se reordena.
    pub fn remap_after_move(&mut self, from: usize, to: usize) {
        for index in &mut self.queued {
            *index = remap_index_after_move(*index, from, to);
        }
        if let Some(index) = &mut self.current {
            *index = remap_index_after_move(*index, from, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queues_only_active_operations_in_order() {
        let mut project = Project::default();
        for _ in 0..4 {
            project.operations.add_new().unwrap();
        }
        project.operations.items[1].active = false;
        let batch = BatchCalculation::from_project(&project).unwrap();
        assert_eq!(batch.queued, VecDeque::from([0, 2, 3]));
        assert_eq!(batch.total, 3);
    }

    #[test]
    fn no_active_operations_means_no_batch() {
        let mut project = Project::default();
        project.operations.add_new().unwrap();
        project.operations.items[0].active = false;
        assert!(BatchCalculation::from_project(&project).is_none());
    }

    #[test]
    fn reordering_remaps_active_and_pending_indices() {
        let mut project = Project::default();
        for _ in 0..4 {
            project.operations.add_new().unwrap();
        }
        let mut batch = BatchCalculation::from_project(&project).unwrap();
        batch.current = batch.queued.pop_front();
        batch.remap_after_move(0, 3);
        assert_eq!(batch.current, Some(3));
        assert_eq!(batch.queued, VecDeque::from([0, 1, 2]));
    }
}

//! Planificacion neutral de las transiciones entre operaciones CAM.
//! No emite G-code ni infiere el estado fisico de la maquina o de M6.

use crate::cam_contract::ConfiguredCuttingConditions;
use crate::toolpath_links::PlannedMoveKind;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CamMove {
    pub kind: PlannedMoveKind,
    pub from_work_mm: [f64; 3],
    pub to_work_mm: [f64; 3],
    pub pass_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct CamBatchOperation<'a> {
    pub moves_work_mm: &'a [CamMove],
    pub conditions: ConfiguredCuttingConditions,
    pub tool_number: Option<u32>,
    /// Altura de despeje en coordenadas de trabajo, mm.
    pub clearance_z_work_mm: f64,
}

pub(crate) struct PlannedBatchOperation<'batch, 'moves> {
    pub(crate) operation: &'batch CamBatchOperation<'moves>,
    /// Se preservan las tres etapas, omitiendo exclusivamente las nulas.
    pub(crate) transition: [Option<CamMove>; 3],
}

pub(crate) struct BatchPlan<'batch, 'moves> {
    pub(crate) operations: Vec<PlannedBatchOperation<'batch, 'moves>>,
}

fn rapid(from: [f64; 3], to: [f64; 3]) -> Option<CamMove> {
    (from != to).then_some(CamMove {
        kind: PlannedMoveKind::RapidTraverse,
        from_work_mm: from,
        to_work_mm: to,
        pass_index: 0,
    })
}

fn transition(previous: [f64; 3], target: [f64; 3]) -> [Option<CamMove>; 3] {
    let link_z = previous[2].max(target[2]);
    let vertical_top = [previous[0], previous[1], link_z];
    let horizontal_end = [target[0], target[1], link_z];
    [
        rapid(previous, vertical_top),
        rapid(vertical_top, horizontal_end),
        rapid(horizontal_end, target),
    ]
}

/// Requiere operaciones previamente verificadas y no vacias. Solo planifica
/// los enlaces; cada trayectoria propia conserva sus movimientos y su orden.
/// Ningun enlace se redondea aqui: eso corresponde al postprocesador.
pub(crate) fn plan_batch<'batch, 'moves>(
    operations: &'batch [CamBatchOperation<'moves>],
) -> BatchPlan<'batch, 'moves> {
    let mut previous = None;
    let mut planned = Vec::with_capacity(operations.len());
    for operation in operations {
        let first = operation
            .moves_work_mm
            .first()
            .expect("El post ha validado cada trayectoria no vacia");
        let links = match previous {
            Some(position) => transition(position, first.from_work_mm),
            None => [None; 3],
        };
        planned.push(PlannedBatchOperation {
            operation,
            transition: links,
        });
        previous = operation
            .moves_work_mm
            .last()
            .map(|movement| movement.to_work_mm);
    }
    BatchPlan {
        operations: planned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op<'a>(moves: &'a [CamMove], clearance: f64) -> CamBatchOperation<'a> {
        CamBatchOperation {
            moves_work_mm: moves,
            conditions: ConfiguredCuttingConditions {
                feed_mm_min: 100.0,
                spindle_rpm: 8000.0,
            },
            tool_number: None,
            clearance_z_work_mm: clearance,
        }
    }

    fn path(at: [f64; 3], tip: [f64; 3]) -> [CamMove; 2] {
        [
            CamMove {
                kind: PlannedMoveKind::Approach,
                from_work_mm: at,
                to_work_mm: tip,
                pass_index: 11,
            },
            CamMove {
                kind: PlannedMoveKind::Retract,
                from_work_mm: tip,
                to_work_mm: at,
                pass_index: 11,
            },
        ]
    }

    #[test]
    fn one_operation_has_no_transition_and_keeps_source_moves() {
        let a = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
        let ops = [op(&a, 12.0)];
        let plan = plan_batch(&ops);
        assert_eq!(plan.operations.len(), 1);
        assert!(plan.operations[0].transition.iter().all(Option::is_none));
        assert_eq!(plan.operations[0].operation.moves_work_mm, &a);
    }

    #[test]
    fn different_clearances_keep_raise_xy_lower_order() {
        let a = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
        let b = path([5.0, 7.0, 20.0], [5.0, 7.0, 2.0]);
        let c = path([9.0, 7.0, 10.0], [9.0, 7.0, 3.0]);
        let ops = [op(&a, 12.0), op(&b, 20.0), op(&c, 10.0)];
        let plan = plan_batch(&ops);
        assert_eq!(
            plan.operations[1].transition,
            [
                rapid([0.0, 0.0, 12.0], [0.0, 0.0, 20.0]),
                rapid([0.0, 0.0, 20.0], [5.0, 7.0, 20.0]),
                None,
            ]
        );
        assert_eq!(
            plan.operations[2].transition,
            [
                None,
                rapid([5.0, 7.0, 20.0], [9.0, 7.0, 20.0]),
                rapid([9.0, 7.0, 20.0], [9.0, 7.0, 10.0]),
            ]
        );
        assert_eq!(plan.operations[1].operation.moves_work_mm, &b);
        assert_eq!(plan.operations[2].operation.moves_work_mm, &c);
    }

    #[test]
    fn coincident_endpoints_do_not_add_zero_length_motion() {
        let a = path([0.0, 0.0, 10.0], [0.0, 0.0, 1.0]);
        let ops = [op(&a, 10.0), op(&a, 10.0)];
        let plan = plan_batch(&ops);
        assert_eq!(plan.operations[1].transition, [None; 3]);
    }
}

//! Regresiones de NC conjunto: golden files codificados como literales.
//! Los textos esperados proceden de la semantica del post anterior a la
//! extraccion del planificador, no de una segunda ejecucion del plan nuevo.

use cam_libre_visor::cam_contract::ConfiguredCuttingConditions;
use cam_libre_visor::nc_post::{
    PostBatchOperation, PostError, PostMove, PostProcessorProfile, PostToolChange,
    generate_batch_program,
};
use cam_libre_visor::toolpath_links::PlannedMoveKind;

fn path(at: [f64; 3], tip: [f64; 3]) -> [PostMove; 2] {
    [
        PostMove {
            kind: PlannedMoveKind::Approach,
            from_work_mm: at,
            to_work_mm: tip,
            pass_index: 0,
        },
        PostMove {
            kind: PlannedMoveKind::Retract,
            from_work_mm: tip,
            to_work_mm: at,
            pass_index: 0,
        },
    ]
}

fn op<'a>(
    moves: &'a [PostMove],
    t: Option<u32>,
    feed: f64,
    clearance: f64,
) -> PostBatchOperation<'a> {
    PostBatchOperation {
        moves_work_mm: moves,
        conditions: ConfiguredCuttingConditions {
            feed_mm_min: feed,
            spindle_rpm: 8000.0,
        },
        tool_number: t,
        clearance_z_work_mm: clearance,
    }
}

fn m6_profile() -> PostProcessorProfile {
    let mut profile = PostProcessorProfile::grbl_3axis_mm();
    profile.tool_change = PostToolChange::M6;
    profile
}

#[test]
fn golden_one_operation_no_t() {
    let a = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
    let generated = generate_batch_program(
        &PostProcessorProfile::grbl_3axis_mm(),
        &[op(&a, None, 100.0, 12.0)],
    )
    .unwrap();
    let expected = concat!(
        "G21\nG90\nG94\nG17\nS8000\nM3\n",
        "G1 X0.000 Y0.000 Z1.000 F100.000\n",
        "G1 X0.000 Y0.000 Z12.000\nM5\nM30\n",
    );
    assert_eq!(generated.text, expected);
    assert_eq!(generated.cam_move_count, 2);
    assert_eq!(
        generated.required_first_move_position_work_mm,
        [0.0, 0.0, 12.0]
    );
    assert_eq!(generated.last_cam_position_work_mm, [0.0, 0.0, 12.0]);
}

#[test]
fn golden_two_operations_repeated_t_and_higher_clearance() {
    let a = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
    let b = path([5.0, 0.0, 20.0], [5.0, 0.0, 2.0]);
    let generated = generate_batch_program(
        &m6_profile(),
        &[op(&a, Some(3), 100.0, 12.0), op(&b, Some(3), 220.0, 20.0)],
    )
    .unwrap();
    let expected = concat!(
        "G21\nG90\nG94\nG17\nT3 M6\nS8000\nM3\n",
        "G1 X0.000 Y0.000 Z1.000 F100.000\n",
        "G1 X0.000 Y0.000 Z12.000\n",
        "G0 X0.000 Y0.000 Z20.000\n",
        "G0 X5.000 Y0.000 Z20.000\n",
        "M5\nT3 M6\nS8000\nM3\n",
        "G1 X5.000 Y0.000 Z2.000 F220.000\n",
        "G1 X5.000 Y0.000 Z20.000\nM5\nM30\n",
    );
    assert_eq!(generated.text, expected);
    assert_eq!(generated.cam_move_count, 6);
    assert_eq!(generated.last_cam_position_work_mm, [5.0, 0.0, 20.0]);
}

#[test]
fn golden_distinct_and_absent_t_with_lower_clearance() {
    let a = path([0.0, 0.0, 20.0], [0.0, 0.0, 1.0]);
    let b = path([5.0, 0.0, 10.0], [5.0, 0.0, 2.0]);
    let generated = generate_batch_program(
        &m6_profile(),
        &[
            op(&a, Some(1), 100.0, 20.0),
            op(&b, None, 150.0, 10.0),
            op(&a, Some(5), 200.0, 20.0),
        ],
    )
    .unwrap();
    let expected = concat!(
        "G21\nG90\nG94\nG17\nT1 M6\nS8000\nM3\n",
        "G1 X0.000 Y0.000 Z1.000 F100.000\n",
        "G1 X0.000 Y0.000 Z20.000\n",
        "G0 X5.000 Y0.000 Z20.000\n",
        "G0 X5.000 Y0.000 Z10.000\n",
        "S8000\nM3\n",
        "G1 X5.000 Y0.000 Z2.000 F150.000\n",
        "G1 X5.000 Y0.000 Z10.000\n",
        "G0 X5.000 Y0.000 Z20.000\n",
        "G0 X0.000 Y0.000 Z20.000\n",
        "M5\nT5 M6\nS8000\nM3\n",
        "G1 X0.000 Y0.000 Z1.000 F200.000\n",
        "G1 X0.000 Y0.000 Z20.000\nM5\nM30\n",
    );
    assert_eq!(generated.text, expected);
    assert_eq!(generated.cam_move_count, 10);
}

#[test]
fn unsupported_m6_and_custom_start_end_keep_rejections() {
    let a = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
    let with_t = [op(&a, Some(7), 100.0, 12.0)];
    assert_eq!(
        generate_batch_program(&PostProcessorProfile::grbl_3axis_mm(), &with_t).unwrap_err(),
        PostError::UnsupportedToolChange
    );
    let no_t = [op(&a, None, 100.0, 12.0)];
    let mut profile = m6_profile();
    profile.start_commands = "(Cabecera)".into();
    assert_eq!(
        generate_batch_program(&profile, &no_t).unwrap_err(),
        PostError::UnsafeBatchLink
    );
    profile.start_commands.clear();
    profile.end_commands = "M9".into();
    assert_eq!(
        generate_batch_program(&profile, &no_t).unwrap_err(),
        PostError::UnsafeBatchLink
    );
}

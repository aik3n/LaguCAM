//! Regresiones de NC conjunto: golden files codificados como literales.
//! Los textos esperados proceden de la semantica del post anterior a la
//! extraccion del planificador, no de una segunda ejecucion del plan nuevo.

use cam_libre_visor::cam_contract::ConfiguredCuttingConditions;
use cam_libre_visor::nc_post::{
    PostBatchOperation, PostError, PostInput, PostLineEnding, PostLineNumbers, PostMove,
    PostProcessorProfile, PostToolChange, PostUnits, generate_batch_program, generate_program,
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

// Referencias fijadas sobre el comportamiento del SHA 6325e521, antes de
// unificar los dos emisores. No se derivan del nuevo motor comun.
#[test]
fn golden_single_and_batch_one_operation_share_text_without_custom_blocks() {
    let moves = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
    let profile = PostProcessorProfile::grbl_3axis_mm();
    let individual = generate_program(
        &profile,
        PostInput {
            moves_work_mm: &moves,
            conditions: ConfiguredCuttingConditions {
                feed_mm_min: 100.0,
                spindle_rpm: 8000.0,
            },
        },
    )
    .unwrap();
    let combined = generate_batch_program(&profile, &[op(&moves, None, 100.0, 12.0)]).unwrap();
    let expected = concat!(
        "G21\nG90\nG94\nG17\nS8000\nM3\n",
        "G1 X0.000 Y0.000 Z1.000 F100.000\n",
        "G1 X0.000 Y0.000 Z12.000\nM5\nM30\n",
    );
    assert_eq!(individual, combined);
    assert_eq!(individual.text, expected);
    assert_eq!(individual.cam_move_count, 2);
    assert!(!individual.has_custom_blocks);
}

#[test]
fn golden_single_start_end_inches_crlf_modal_precision_and_line_numbers() {
    let moves = [
        PostMove {
            kind: PlannedMoveKind::RapidTraverse,
            from_work_mm: [0.0, 0.0, 25.4],
            to_work_mm: [25.4, 0.0, 25.4],
            pass_index: 3,
        },
        PostMove {
            kind: PlannedMoveKind::Approach,
            from_work_mm: [25.4, 0.0, 25.4],
            to_work_mm: [25.4, 0.0, 12.7],
            pass_index: 3,
        },
        PostMove {
            kind: PlannedMoveKind::Cutting,
            from_work_mm: [25.4, 0.0, 12.7],
            to_work_mm: [50.8, 0.0, 12.7],
            pass_index: 3,
        },
        PostMove {
            kind: PlannedMoveKind::Retract,
            from_work_mm: [50.8, 0.0, 12.7],
            to_work_mm: [50.8, 0.0, 25.4],
            pass_index: 3,
        },
    ];
    let mut profile = PostProcessorProfile::grbl_3axis_mm();
    profile.units = PostUnits::Inches;
    profile.line_ending = PostLineEnding::CrLf;
    profile.line_numbers = Some(PostLineNumbers { start: 5, step: 5 });
    profile.suppress_unchanged_axes = true;
    profile.start_commands = "(HEADER)\r\nG91 G1 Z1 F10\r\n".into();
    profile.end_commands = "(END)\nM9\n".into();
    let generated = generate_program(
        &profile,
        PostInput {
            moves_work_mm: &moves,
            conditions: ConfiguredCuttingConditions {
                feed_mm_min: 254.0,
                spindle_rpm: 14000.0,
            },
        },
    )
    .unwrap();
    let expected = concat!(
        "N5 (HEADER)\r\nN10 G91 G1 Z1 F10\r\n",
        "N15 G20\r\nN20 G90\r\nN25 G94\r\nN30 G17\r\n",
        "N35 S14000\r\nN40 M3\r\n",
        "N45 G0 X1.000 Y0.000 Z1.000\r\n",
        "N50 G1 Z0.500 F10.000\r\n",
        "N55 G1 X2.000\r\nN60 G1 Z1.000\r\n",
        "N65 (END)\r\nN70 M9\r\nN75 M5\r\nN80 M30\r\n",
    );
    assert_eq!(generated.text, expected);
    assert_eq!(generated.cam_move_count, 4);
    assert_eq!(
        generated.required_first_move_position_work_mm,
        [0.0, 0.0, 25.4]
    );
    assert_eq!(generated.last_cam_position_work_mm, [50.8, 0.0, 25.4]);
    assert!(generated.has_custom_blocks);
}

#[test]
fn golden_batch_inches_precision_crlf_modal_numbers_and_repeated_t() {
    let a = path([0.0, 0.0, 25.4], [0.0, 0.0, 12.7]);
    let b = path([25.4, 0.0, 50.8], [25.4, 0.0, 0.0]);
    let mut profile = m6_profile();
    profile.units = PostUnits::Inches;
    profile.xyz_decimals = 2;
    profile.feed_decimals = 2;
    profile.line_ending = PostLineEnding::CrLf;
    profile.line_numbers = Some(PostLineNumbers { start: 10, step: 5 });
    profile.suppress_unchanged_axes = true;
    let generated = generate_batch_program(
        &profile,
        &[op(&a, Some(3), 254.0, 25.4), op(&b, Some(3), 508.0, 50.8)],
    )
    .unwrap();
    let expected = concat!(
        "N10 G20\r\nN15 G90\r\nN20 G94\r\nN25 G17\r\n",
        "N30 T3 M6\r\nN35 S8000\r\nN40 M3\r\n",
        "N45 G1 X0.00 Y0.00 Z0.50 F10.00\r\n",
        "N50 G1 Z1.00\r\n",
        "N55 G0 Z2.00\r\nN60 G0 X1.00\r\n",
        "N65 M5\r\nN70 T3 M6\r\nN75 S8000\r\nN80 M3\r\n",
        "N85 G1 Z0.00 F20.00\r\nN90 G1 Z2.00\r\n",
        "N95 M5\r\nN100 M30\r\n",
    );
    assert_eq!(generated.text, expected);
    assert_eq!(generated.cam_move_count, 6);
    assert_eq!(generated.last_cam_position_work_mm, [25.4, 0.0, 50.8]);
    assert!(!generated.has_custom_blocks);
}

#[test]
fn golden_custom_whitespace_differs_between_individual_and_batch() {
    let moves = path([0.0, 0.0, 12.0], [0.0, 0.0, 1.0]);
    let mut profile = PostProcessorProfile::grbl_3axis_mm();
    profile.start_commands = "\n".into();
    let single = generate_program(
        &profile,
        PostInput {
            moves_work_mm: &moves,
            conditions: ConfiguredCuttingConditions {
                feed_mm_min: 100.0,
                spindle_rpm: 8000.0,
            },
        },
    )
    .unwrap();
    let batch = generate_batch_program(&profile, &[op(&moves, None, 100.0, 12.0)]).unwrap();
    assert!(single.text.starts_with("\nG21\n"));
    assert!(single.has_custom_blocks);
    assert!(batch.text.starts_with("G21\n"));
    assert!(!batch.has_custom_blocks);
}

#[test]
fn golden_input_policies_and_error_precedence_are_unchanged() {
    let good = path([0.0, 0.0, 10.0], [0.0, 0.0, 1.0]);
    let empty: [PostMove; 0] = [];
    let profile = PostProcessorProfile::grbl_3axis_mm();
    let bad_conditions = ConfiguredCuttingConditions {
        feed_mm_min: f64::NAN,
        spindle_rpm: 8000.0,
    };
    assert_eq!(
        generate_program(
            &profile,
            PostInput {
                moves_work_mm: &empty,
                conditions: bad_conditions,
            },
        )
        .unwrap_err(),
        PostError::InvalidCuttingConditions,
    );
    let mut bad_profile = profile.clone();
    bad_profile.version = 999;
    assert_eq!(
        generate_program(
            &bad_profile,
            PostInput {
                moves_work_mm: &empty,
                conditions: bad_conditions,
            },
        )
        .unwrap_err(),
        PostError::UnsupportedProfileVersion,
    );
    let mut mixed = good;
    mixed[1].from_work_mm[0] = 99.0;
    assert_eq!(
        generate_program(
            &profile,
            PostInput {
                moves_work_mm: &mixed,
                conditions: ConfiguredCuttingConditions {
                    feed_mm_min: 100.0,
                    spindle_rpm: 8000.0,
                },
            },
        )
        .unwrap_err(),
        PostError::DiscontinuousPath,
    );
    let mut configured = m6_profile();
    configured.start_commands = "(Header)".into();
    assert_eq!(
        generate_batch_program(&configured, &[op(&good, Some(0), 100.0, 10.0)]).unwrap_err(),
        PostError::UnsafeBatchLink,
    );
    configured.start_commands.clear();
    assert_eq!(
        generate_batch_program(&configured, &[op(&good, Some(0), f64::NAN, 10.0)]).unwrap_err(),
        PostError::InvalidToolNumber,
    );
    assert_eq!(
        generate_batch_program(&configured, &[op(&empty, None, 100.0, 0.0)]).unwrap_err(),
        PostError::EmptyToolpath,
    );
    assert_eq!(
        generate_batch_program(&configured, &[op(&good, None, 100.0, 8.0)]).unwrap_err(),
        PostError::UnsafeBatchLink,
    );
    let only_rapid = [PostMove {
        kind: PlannedMoveKind::RapidTraverse,
        from_work_mm: [1.0, 2.0, 3.0],
        to_work_mm: [2.0, 2.0, 3.0],
        pass_index: 0,
    }];
    let single = generate_program(
        &profile,
        PostInput {
            moves_work_mm: &only_rapid,
            conditions: ConfiguredCuttingConditions {
                feed_mm_min: 100.0,
                spindle_rpm: 8000.0,
            },
        },
    )
    .unwrap();
    assert_eq!(
        single.text,
        "G21\nG90\nG94\nG17\nS8000\nM3\nG0 X2.000 Y2.000 Z3.000\nM5\nM30\n"
    );
    assert_eq!(
        generate_batch_program(&profile, &[op(&only_rapid, None, 100.0, 3.0)]).unwrap_err(),
        PostError::UnsafeBatchLink,
    );
}

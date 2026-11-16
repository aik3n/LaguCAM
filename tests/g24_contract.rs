//! G24: contratos publicos del Contact Solver toroidal sobre STL sinteticos.
//! Requieren Windows para el bloqueo de escritura del STL en G17.
#![cfg(windows)]

use cam_libre_visor::cam_geometry::{
    contact_solver::{ContactQuery, ContactSolver, ContactSolverError, CutterGeometry},
    service::{ContactEngineError, ContactEngineLimits},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

struct SyntheticStl(PathBuf);
impl Drop for SyntheticStl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn stl(triangles: &[[[f32; 3]; 3]]) -> SyntheticStl {
    let path = std::env::temp_dir().join(format!("lagucam-g24-{}.stl", uuid::Uuid::new_v4()));
    let mut bytes = vec![0_u8; 80];
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for tri in triangles {
        bytes.extend_from_slice(&[0u8; 12]);
        for point in tri {
            for coordinate in point {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
    }
    fs::write(&path, bytes).unwrap();
    SyntheticStl(path)
}
fn limits() -> ContactEngineLimits {
    ContactEngineLimits {
        max_triangles: 128,
        max_accounted_bytes: 2_000_000,
        cache_bytes: 51_200,
    }
}
fn query(x: f64, y: f64, radius: f64, corner: f64, allowance: f64) -> ContactQuery {
    ContactQuery {
        x_model: x,
        y_model: y,
        cutter: CutterGeometry::BullNose {
            diameter_model: radius * 2.0,
            corner_radius_model: corner,
        },
        allowance_model: allowance,
    }
}
fn solve(file: &SyntheticStl, query: ContactQuery) -> Option<f64> {
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    solver.solve(query, &cancelled).unwrap().tip_z_model
}

#[test]
fn horizontal_matches_nominal_and_uniform_allowance() {
    let file = stl(&[[[0.0, 0.0, 5.0], [100.0, 0.0, 5.0], [0.0, 100.0, 5.0]]]);
    for allowance in [0.0, 1.0, 3.0] {
        for corner in [0.5, 2.0, 6.0] {
            let z = solve(&file, query(10.0, 10.0, 6.0, corner, allowance)).unwrap();
            assert!((z - 5.0 - allowance).abs() < 1e-9);
        }
    }
}

#[test]
fn ramp_compensates_circular_corner_and_allowance_along_normal() {
    let file = stl(&[[[0.0, 0.0, 0.0], [100.0, 0.0, 100.0], [0.0, 100.0, 0.0]]]);
    // z=x, R=6, r=1, K=5. Solucion analitica en la cara:
    // z_tip=x+K+r*(sqrt(2)-1)+a*sqrt(2).
    for allowance in [0.0, 1.0, 3.0] {
        let z = solve(&file, query(10.0, 10.0, 6.0, 1.0, allowance)).unwrap();
        let expected = 15.0 + (2.0_f64.sqrt() - 1.0) + allowance * 2.0_f64.sqrt();
        assert!((z - expected).abs() < 1e-8, "{z} != {expected}");
    }
}

#[test]
fn corner_radius_changes_height_between_flat_and_spherical_limits() {
    let file = stl(&[[[0.0, 0.0, 0.0], [100.0, 0.0, 100.0], [0.0, 100.0, 0.0]]]);
    let small = solve(&file, query(10.0, 10.0, 6.0, 0.5, 0.0)).unwrap();
    let large = solve(&file, query(10.0, 10.0, 6.0, 6.0, 0.0)).unwrap();
    assert!(small > large);
    let expected_small = 10.0 + 5.5 + 0.5 * (2.0_f64.sqrt() - 1.0);
    let expected_large = 10.0 + 6.0 * (2.0_f64.sqrt() - 1.0);
    assert!((small - expected_small).abs() < 1e-8);
    assert!((large - expected_large).abs() < 1e-8);
}

#[test]
fn wall_rim_reduces_tip_and_positive_allowance_expands_reach() {
    let file = stl(&[
        [[6.0, 0.0, 0.0], [6.0, 0.0, 10.0], [6.0, 20.0, 10.0]],
        [[6.0, 0.0, 0.0], [6.0, 20.0, 10.0], [6.0, 20.0, 0.0]],
    ]);
    let nominal = solve(&file, query(0.0, 10.0, 6.0, 2.0, 0.0)).unwrap();
    assert!((nominal - 8.0).abs() < 1e-9);
    assert_eq!(solve(&file, query(-1.0, 10.0, 6.0, 2.0, 0.0)), None);
    assert!(solve(&file, query(-1.0, 10.0, 6.0, 2.0, 1.0)).is_some());
}

#[test]
fn no_contact_invalid_geometry_and_cancellation_are_typed() {
    let file = stl(&[[[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [0.0, 20.0, 5.0]]]);
    let flag = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &flag).unwrap();
    assert_eq!(
        solver
            .solve(query(100.0, 100.0, 6.0, 1.0, 0.0), &flag)
            .unwrap()
            .tip_z_model,
        None
    );
    for bad in [0.0, -1.0, 7.0, f64::NAN] {
        assert!(matches!(
            solver.solve(query(5.0, 5.0, 6.0, bad, 0.0), &flag),
            Err(ContactSolverError::InvalidQuery(_))
        ));
    }
    assert!(matches!(
        solver.solve(query(5.0, 5.0, 6.0, 1.0, -1.0), &flag),
        Err(ContactSolverError::InvalidQuery(_))
    ));
    flag.store(true, Ordering::Relaxed);
    assert!(matches!(
        solver.solve(query(5.0, 5.0, 6.0, 1.0, 0.0), &flag),
        Err(ContactSolverError::Engine(ContactEngineError::Cancelled))
    ));
    flag.store(false, Ordering::Relaxed);
    assert!(
        solver
            .solve(query(5.0, 5.0, 6.0, 1.0, 0.0), &flag)
            .unwrap()
            .tip_z_model
            .is_some()
    );
}

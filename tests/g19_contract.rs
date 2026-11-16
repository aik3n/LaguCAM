//! G19: contrato de Contact Solver y sobrematerial uniforme sobre STL sinteticos.
//! Estas pruebas requieren Windows porque el servicio de archivo protegido G17
//! todavia no esta implementado para otras plataformas.
#![cfg(windows)]

use cam_libre_visor::cam_geometry::{
    contact_solver::{ContactQuery, ContactSolver, ContactSolverError, CutterGeometry},
    service::{BallContactEngine, BallContactQuery, ContactEngineError, ContactEngineLimits},
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct SyntheticStl(PathBuf);
impl Drop for SyntheticStl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn stl(triangles: &[[[f32; 3]; 3]]) -> SyntheticStl {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "lagucam_g19_{}_{}.stl",
        std::process::id(),
        counter
    ));
    let mut bytes = vec![0_u8; 80];
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for tri in triangles {
        bytes.extend_from_slice(&[0_u8; 12]);
        for vertex in tri {
            for coordinate in vertex {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0_u8; 2]);
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
fn sphere(x: f64, y: f64, diameter: f64, allowance: f64) -> ContactQuery {
    ContactQuery {
        x_model: x,
        y_model: y,
        cutter: CutterGeometry::BallNose {
            diameter_model: diameter,
        },
        allowance_model: allowance,
    }
}

#[test]
fn zero_allowance_matches_previous_public_engine_bit_for_bit() {
    let triangles = [
        [[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [0.0, 20.0, 5.0]],
        [[0.0, 0.0, 0.0], [20.0, 0.0, 20.0], [0.0, 20.0, 0.0]],
    ];
    let file = stl(&triangles);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    let (mut old, _) = BallContactEngine::open_stl(&file.0, limits(), &cancelled).unwrap();
    for (x, y) in [(2.0, 2.0), (5.0, 5.0), (100.0, 100.0)] {
        let result = solver.solve(sphere(x, y, 4.0, 0.0), &cancelled).unwrap();
        let legacy = old
            .query(
                BallContactQuery {
                    x_model: x,
                    y_model: y,
                    radius_model: 2.0,
                },
                &cancelled,
            )
            .unwrap();
        assert_eq!(
            result.tip_z_model.map(f64::to_bits),
            legacy.tip_z_model.map(f64::to_bits)
        );
        assert_eq!(result.triangles_considered, legacy.triangles_considered);
    }
}

#[test]
fn horizontal_plane_clearance_is_exactly_uniform_allowance() {
    let file = stl(&[[[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [0.0, 20.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    for allowance in [0.0, 1.0, 3.0] {
        let result = solver
            .solve(sphere(5.0, 5.0, 4.0, allowance), &cancelled)
            .unwrap();
        assert!((result.tip_z_model.unwrap() - (5.0 + allowance)).abs() < 1.0e-10);
    }
}

#[test]
fn ramp_clearance_uses_surface_normal_not_a_vertical_z_offset() {
    // Plano z = x, pendiente 45 grados, suficiente extension para contacto interior.
    let file = stl(&[[[0.0, 0.0, 0.0], [20.0, 0.0, 20.0], [0.0, 20.0, 0.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    let z0 = solver
        .solve(sphere(3.0, 3.0, 4.0, 0.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    let z1 = solver
        .solve(sphere(3.0, 3.0, 4.0, 1.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    let expected_nominal = 3.0 + 2.0 * (2.0_f64.sqrt() - 1.0);
    assert!((z0 - expected_nominal).abs() < 1.0e-10);
    assert!((z1 - z0 - 2.0_f64.sqrt()).abs() < 1.0e-10);
}

#[test]
fn no_contact_remains_none_and_not_a_clearance_height() {
    let file = stl(&[[[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    assert!(
        solver
            .solve(sphere(100.0, 100.0, 4.0, 1.0), &cancelled)
            .unwrap()
            .tip_z_model
            .is_none()
    );
}

#[test]
fn rejects_invalid_and_unsupported_geometries_without_approximations() {
    let file = stl(&[[[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [0.0, 20.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    assert!(matches!(
        solver.solve(sphere(5.0, 5.0, 4.0, -1.0), &cancelled),
        Err(ContactSolverError::InvalidQuery(_))
    ));
    assert!(matches!(
        solver.solve(sphere(f64::NAN, 5.0, 4.0, 0.0), &cancelled),
        Err(ContactSolverError::InvalidQuery(_))
    ));
    assert!(matches!(
        solver.solve(sphere(5.0, 5.0, f64::MAX, 0.0), &cancelled),
        Err(ContactSolverError::ArithmeticRange)
    ));
    for cutter in [
        CutterGeometry::VBit {
            diameter_model: 6.0,
            included_angle_deg: 60.0,
        },
        CutterGeometry::ConicalFlat {
            diameter_model: 8.0,
            tip_diameter_model: 1.0,
            included_angle_deg: 60.0,
        },
    ] {
        assert!(matches!(
            solver.solve(
                ContactQuery {
                    x_model: 5.0,
                    y_model: 5.0,
                    cutter,
                    allowance_model: 0.0,
                },
                &cancelled
            ),
            Err(ContactSolverError::UnsupportedGeometry)
        ));
    }
}

#[test]
fn cancellation_is_typed_and_solver_is_reusable() {
    let file = stl(&[[[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [0.0, 20.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancelled).unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(
        solver.solve(sphere(5.0, 5.0, 4.0, 0.0), &cancelled),
        Err(ContactSolverError::Engine(ContactEngineError::Cancelled))
    ));
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(
        solver
            .solve(sphere(5.0, 5.0, 4.0, 0.0), &cancelled)
            .unwrap()
            .tip_z_model,
        Some(5.0)
    );
}

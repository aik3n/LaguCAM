//! G22: sobrematerial euclideo de fresa plana sobre STL sinteticos.
//! Solo Windows: G17 mantiene abierto el archivo sin compartir escritura.
#![cfg(windows)]

use cam_libre_visor::cam_geometry::{
    contact_solver::{ContactQuery, ContactSolver, ContactSolverError, CutterGeometry},
    service::{ContactEngineError, ContactEngineLimits},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct TempStl(PathBuf);
impl Drop for TempStl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn stl(triangles: &[[[f32; 3]; 3]]) -> TempStl {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("lagucam_g22_{}_{}.stl", std::process::id(), id));
    let mut bytes = vec![0_u8; 80];
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for tri in triangles {
        bytes.extend_from_slice(&[0_u8; 12]);
        for vertex in tri {
            for component in vertex {
                bytes.extend_from_slice(&component.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0_u8; 2]);
    }
    fs::write(&path, bytes).unwrap();
    TempStl(path)
}
fn limits() -> ContactEngineLimits {
    ContactEngineLimits {
        max_triangles: 128,
        max_accounted_bytes: 2_000_000,
        cache_bytes: 51_200,
    }
}
fn flat(x: f64, y: f64, diameter: f64, allowance: f64) -> ContactQuery {
    ContactQuery {
        x_model: x,
        y_model: y,
        cutter: CutterGeometry::Flat {
            diameter_model: diameter,
        },
        allowance_model: allowance,
    }
}

#[test]
fn horizontal_plane_is_offset_exactly_by_allowance() {
    let file = stl(&[[[0.0, 0.0, 5.0], [50.0, 0.0, 5.0], [0.0, 50.0, 5.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    for diameter in [2.0, 12.0] {
        for allowance in [0.0, 1.0, 3.0] {
            let z = solver
                .solve(flat(8.0, 8.0, diameter, allowance), &cancel)
                .unwrap()
                .tip_z_model
                .unwrap();
            assert!(
                (z - (5.0 + allowance)).abs() < 1e-10,
                "diameter={diameter} a={allowance} z={z}"
            );
        }
    }
}

#[test]
fn slope_allowance_uses_euclidean_normal_not_just_z() {
    // z=x. Contacto plano nominal x+r; offset geometrico x+r+sqrt(2)*a.
    let file = stl(&[[[0.0, 0.0, 0.0], [60.0, 0.0, 60.0], [0.0, 60.0, 0.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    for diameter in [2.0, 12.0] {
        for allowance in [0.5, 1.0, 3.0] {
            let z = solver
                .solve(flat(10.0, 10.0, diameter, allowance), &cancel)
                .unwrap()
                .tip_z_model
                .unwrap();
            let expected = 10.0 + diameter / 2.0 + 2.0_f64.sqrt() * allowance;
            assert!(
                (z - expected).abs() < 1e-9,
                "d={diameter} a={allowance} z={z} expected={expected}"
            );
        }
    }
}

#[test]
fn vertical_wall_has_rounded_offset_in_xy() {
    // Cara vertical x=5, arista superior horizontal z=10.
    // Eje en x=0; con radio 4, la distancia horizontal al disco es 1.
    let file = stl(&[[[5.0, -20.0, 10.0], [5.0, 20.0, 10.0], [5.0, 0.0, 0.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    assert_eq!(
        solver
            .solve(flat(0.0, 0.0, 8.0, 0.0), &cancel)
            .unwrap()
            .tip_z_model,
        None
    );
    let tangent = solver
        .solve(flat(0.0, 0.0, 8.0, 1.0), &cancel)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((tangent - 10.0).abs() < 1e-10);
    let rounded = solver
        .solve(flat(0.0, 0.0, 8.0, 2.0), &cancel)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((rounded - (10.0 + 3.0_f64.sqrt())).abs() < 1e-10);
}

#[test]
fn no_contact_outside_expanded_radius_remains_none() {
    let file = stl(&[[[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    assert!(
        solver
            .solve(flat(100.0, 100.0, 4.0, 3.0), &cancel)
            .unwrap()
            .tip_z_model
            .is_none()
    );
}

#[test]
fn zero_allowance_keeps_g21_flat_solution_bit_for_bit() {
    let file = stl(&[[[0.0, 0.0, 0.0], [40.0, 0.0, 40.0], [0.0, 40.0, 0.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    for (x, y, diameter, expected) in [
        (8.0, 8.0, 2.0, Some(9.0)),
        (8.0, 8.0, 12.0, Some(14.0)),
        (100.0, 100.0, 4.0, None),
    ] {
        let got = solver
            .solve(flat(x, y, diameter, 0.0), &cancel)
            .unwrap()
            .tip_z_model;
        assert_eq!(got.map(f64::to_bits), expected.map(f64::to_bits));
    }
}

#[test]
fn invalid_allowance_never_produces_partial_height() {
    let file = stl(&[[[0.0, 0.0, 0.0], [30.0, 0.0, 30.0], [0.0, 30.0, 0.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    for allowance in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            solver.solve(flat(5.0, 5.0, 4.0, allowance), &cancel),
            Err(ContactSolverError::InvalidQuery(_))
        ));
    }
    assert!(matches!(
        solver.solve(flat(5.0, 5.0, f64::MAX, f64::MAX), &cancel),
        Err(ContactSolverError::ArithmeticRange)
    ));
}

#[test]
fn cancellation_and_engine_reuse_are_preserved() {
    let file = stl(&[[[0.0, 0.0, 5.0], [30.0, 0.0, 5.0], [0.0, 30.0, 5.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut solver, _) = ContactSolver::open_stl(&file.0, limits(), &cancel).unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert!(matches!(
        solver.solve(flat(5.0, 5.0, 4.0, 2.0), &cancel),
        Err(ContactSolverError::Engine(ContactEngineError::Cancelled))
    ));
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        solver
            .solve(flat(5.0, 5.0, 4.0, 2.0), &cancel)
            .unwrap()
            .tip_z_model,
        Some(7.0)
    );
}

//! G21: contrato independiente para fresa plana sobre STL sinteticos.
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

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
struct TempStl(PathBuf);
impl Drop for TempStl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn write_stl(triangles: &[[[f32; 3]; 3]]) -> TempStl {
    let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("lagucam_g21_{}_{}.stl", std::process::id(), unique));
    let mut bytes = vec![0_u8; 80];
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for triangle in triangles {
        bytes.extend_from_slice(&[0_u8; 12]);
        for vertex in triangle {
            for v in vertex {
                bytes.extend_from_slice(&v.to_le_bytes());
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
fn horizontal_plane_is_independent_of_flat_diameter() {
    let stl = write_stl(&[[[0.0, 0.0, 5.0], [40.0, 0.0, 5.0], [0.0, 40.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    for diameter in [2.0, 12.0] {
        assert_eq!(
            engine
                .solve(flat(8.0, 8.0, diameter, 0.0), &cancelled)
                .unwrap()
                .tip_z_model,
            Some(5.0)
        );
    }
}

#[test]
fn slope_responds_to_flat_diameter_exactly() {
    // z=x, la cota superior bajo el disco es x+r: diametros 2 y 12.
    let stl = write_stl(&[[[0.0, 0.0, 0.0], [40.0, 0.0, 40.0], [0.0, 40.0, 0.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    let z2 = engine
        .solve(flat(8.0, 8.0, 2.0, 0.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    let z12 = engine
        .solve(flat(8.0, 8.0, 12.0, 0.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((z2 - 9.0).abs() < 1e-10, "z2={z2}");
    assert!((z12 - 14.0).abs() < 1e-10, "z12={z12}");
}

#[test]
fn disk_intersects_edges_even_when_every_vertex_is_outside() {
    // Triangulo largo y fino. La frontera del disco cruza una arista,
    // aunque no hay un solo vertice dentro.
    let stl = write_stl(&[[[-10.0, 0.0, -10.0], [10.0, 0.0, 10.0], [0.0, 10.0, 0.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    let z = engine
        .solve(flat(0.0, 0.0, 4.0, 0.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((z - 2.0).abs() < 1e-10, "z={z}");
}

#[test]
fn vertical_face_rim_contact_uses_upper_edge() {
    // Plano vertical x=5, punto tangente al disco de radio 2.
    let stl = write_stl(&[[[5.0, 0.0, 0.0], [5.0, 0.0, 10.0], [5.0, 10.0, 4.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    let z = engine
        .solve(flat(3.0, 5.0, 4.0, 0.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((z - 7.0).abs() < 1e-10, "z={z}");
}

#[test]
fn missing_contact_and_cancel_are_not_heights() {
    let stl = write_stl(&[[[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    assert!(
        engine
            .solve(flat(100.0, 100.0, 4.0, 0.0), &cancelled)
            .unwrap()
            .tip_z_model
            .is_none()
    );
    cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(
        engine.solve(flat(5.0, 5.0, 4.0, 0.0), &cancelled),
        Err(ContactSolverError::Engine(ContactEngineError::Cancelled))
    ));
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(
        engine
            .solve(flat(3.0, 3.0, 4.0, 0.0), &cancelled)
            .unwrap()
            .tip_z_model,
        Some(5.0)
    );
}

#[test]
fn flat_allowance_uses_geometric_offset_not_vertical_approximation() {
    let stl = write_stl(&[[[0.0, 0.0, 0.0], [30.0, 0.0, 30.0], [0.0, 30.0, 0.0]]]);
    let cancelled = AtomicBool::new(false);
    let (mut engine, _) = ContactSolver::open_stl(&stl.0, limits(), &cancelled).unwrap();
    let z = engine
        .solve(flat(5.0, 5.0, 4.0, 1.0), &cancelled)
        .unwrap()
        .tip_z_model
        .unwrap();
    assert!((z - (7.0 + 2.0_f64.sqrt())).abs() < 1e-10, "z={z}");
    assert!(matches!(
        engine.solve(flat(5.0, 5.0, 0.0, 0.0), &cancelled),
        Err(ContactSolverError::InvalidQuery(_))
    ));
    assert!(matches!(
        engine.solve(flat(f64::NAN, 5.0, 4.0, 0.0), &cancelled),
        Err(ContactSolverError::InvalidQuery(_))
    ));
}

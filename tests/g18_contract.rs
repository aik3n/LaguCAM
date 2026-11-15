//! G18: pruebas de aceptación de la API pública G17, sin UI ni trayectorias.
//! En Windows se comprueba también la protección del fichero STL abierto.
#![cfg(windows)]

use cam_libre_visor::cam_geometry::{
    Vec3, ball_tip_height_mesh,
    service::{BallContactEngine, BallContactQuery, ContactEngineError, ContactEngineLimits},
    stl::{BuildControl, FileBvh},
};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TemporaryStl(PathBuf);

impl Drop for TemporaryStl {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn temporary_stl(triangles: &[[[f32; 3]; 3]]) -> TemporaryStl {
    let number = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "lagucam_g18_contract_{}_{}.stl",
        std::process::id(),
        number
    ));
    let mut data = vec![0_u8; 80];
    data.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for tri in triangles {
        data.extend_from_slice(&[0_u8; 12]); // normal STL: no se usa para CAM
        for vertex in tri {
            for axis in vertex {
                data.extend_from_slice(&axis.to_le_bytes());
            }
        }
        data.extend_from_slice(&[0_u8; 2]); // atributo STL
    }
    fs::write(&path, data).expect("escribir STL temporal sintetico");
    TemporaryStl(path)
}

fn limits() -> ContactEngineLimits {
    ContactEngineLimits {
        max_triangles: 128,
        max_accounted_bytes: 2_000_000,
        cache_bytes: 1024 * 50,
    }
}

fn query(x: f64, y: f64, radius: f64) -> BallContactQuery {
    BallContactQuery {
        x_model: x,
        y_model: y,
        radius_model: radius,
    }
}

fn reference_triangles(triangles: &[[[f32; 3]; 3]]) -> Vec<[Vec3; 3]> {
    triangles
        .iter()
        .map(|triangle| {
            (*triangle).map(|v| Vec3 {
                x: f64::from(v[0]),
                y: f64::from(v[1]),
                z: f64::from(v[2]),
            })
        })
        .collect()
}

fn identical_optional_f64(actual: Option<f64>, expected: Option<f64>) -> bool {
    actual.map(f64::to_bits) == expected.map(f64::to_bits)
}

#[test]
fn g18_public_api_matches_exhaustive_and_g16_bitwise() {
    let triangles = [
        [[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]],
        [[0.0, 0.0, 1.0], [8.0, 0.0, 6.0], [0.0, 8.0, 3.0]],
        [[4.0, 4.0, 0.0], [4.0, 4.0, 12.0], [4.0, 10.0, 3.0]],
        [[20.0, 20.0, 7.0], [25.0, 20.0, 7.0], [20.0, 25.0, 7.0]],
    ];
    let file = temporary_stl(&triangles);
    let cancel = AtomicBool::new(false);
    let (mut service, report) = BallContactEngine::open_stl(&file.0, limits(), &cancel).unwrap();
    assert_eq!(report.triangles, triangles.len());
    assert!(report.nodes > 0);
    assert!(report.memory.accounted_bytes <= report.memory.configured_budget_bytes);
    let control = BuildControl {
        max_triangles: 128,
        max_index_bytes: 2_000_000,
        cancel_file: None,
    };
    let (mut bvh, _) = FileBvh::build(&file.0, &control).unwrap();
    bvh.configure_g16_auto_cache(1024 * 50, 2_000_000).unwrap();
    let reference = reference_triangles(&triangles);
    let positions = [
        (2.0, 2.0, 1.0),
        (4.0, 4.0, 2.0),
        (7.0, 1.0, 3.0),
        (22.0, 22.0, 2.0),
        (100.0, 100.0, 1.0),
    ];
    for (x, y, radius) in positions {
        let public = service.query(query(x, y, radius), &cancel).unwrap();
        let exhaustive = ball_tip_height_mesh(&reference, x, y, radius).unwrap();
        let indexed = bvh.query_g16_auto(x, y, radius).unwrap().0;
        assert!(identical_optional_f64(public.tip_z_model, exhaustive));
        assert!(identical_optional_f64(public.tip_z_model, indexed));
    }
}

#[test]
fn g18_rejects_invalid_queries_and_no_contact_is_not_error() {
    let file = temporary_stl(&[[[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut engine, _) = BallContactEngine::open_stl(&file.0, limits(), &cancel).unwrap();
    assert_eq!(
        engine
            .query(query(100.0, 100.0, 1.0), &cancel)
            .unwrap()
            .tip_z_model,
        None
    );
    for bad in [
        query(f64::NAN, 1.0, 1.0),
        query(1.0, f64::INFINITY, 1.0),
        query(1.0, 1.0, -1.0),
        query(1.0, 1.0, 0.0),
    ] {
        assert!(matches!(
            engine.query(bad, &cancel),
            Err(ContactEngineError::InvalidQuery)
        ));
    }
}

#[test]
fn g18_cancel_returns_no_partial_result_and_engine_can_be_reused() {
    let file = temporary_stl(&[[[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]]]);
    let cancel = AtomicBool::new(false);
    let (mut engine, _) = BallContactEngine::open_stl(&file.0, limits(), &cancel).unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert!(matches!(
        engine.query(query(2.0, 2.0, 1.0), &cancel),
        Err(ContactEngineError::Cancelled)
    ));
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        engine
            .query(query(2.0, 2.0, 1.0), &cancel)
            .unwrap()
            .tip_z_model,
        Some(5.0)
    );
}

#[test]
fn g18_windows_handle_prevents_writer_until_engine_is_dropped() {
    let file = temporary_stl(&[[[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]]]);
    let cancel = AtomicBool::new(false);
    let (engine, _) = BallContactEngine::open_stl(&file.0, limits(), &cancel).unwrap();
    assert!(OpenOptions::new().write(true).open(&file.0).is_err());
    drop(engine);
    let writer = OpenOptions::new().write(true).open(&file.0).unwrap();
    drop(writer);
}

#[test]
fn g18_rejects_malformed_stl_and_budget_excess_with_typed_error() {
    let file = temporary_stl(&[[[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]]]);
    let cancel = AtomicBool::new(false);
    let constrained = ContactEngineLimits {
        max_accounted_bytes: 1,
        ..limits()
    };
    assert!(matches!(
        BallContactEngine::open_stl(&file.0, constrained, &cancel),
        Err(ContactEngineError::InvalidStlOrLimit(_))
    ));
    let empty = temporary_stl(&[]);
    assert!(matches!(
        BallContactEngine::open_stl(&empty.0, limits(), &cancel),
        Err(ContactEngineError::InvalidStlOrLimit(_))
    ));
}

#[test]
fn g18_missing_stl_returns_io_error() {
    let number = COUNTER.fetch_add(1, Ordering::Relaxed);
    let missing = std::env::temp_dir().join(format!(
        "lagucam_g18_absent_{}_{}.stl",
        std::process::id(),
        number
    ));
    assert!(!Path::new(&missing).exists());
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        BallContactEngine::open_stl(&missing, limits(), &cancel),
        Err(ContactEngineError::Io(_))
    ));
}

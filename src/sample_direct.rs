//! Cinco perfiles de demostración calculados directamente sobre el STL abierto.
//! Solo visualización: no genera movimientos CAM, trayectorias ni G-code.
use crate::{
    sample_overlay::{PASSES, Profiles, SAMPLES_PER_PASS},
    stl::Bounds,
};
use cam_libre_visor::cam_geometry::service::{
    BallContactEngine, BallContactQuery, ContactEngineLimits,
};
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
};

const MAX_TRIANGLES: usize = 21_000_000;
const MAX_ACCOUNTED_BYTES: usize = 700 * 1024 * 1024;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const BALL_RADIUS: f64 = 2.0; // Unidades del STL: el formato no declara milímetros.

pub struct SampleOutcome {
    pub profiles: Profiles,
    pub contact_count: usize,
}

/// Mantiene la recepción y la cancelación cooperativa del trabajo en segundo plano.
/// Al descartar el trabajo (por ejemplo al abrir otro STL) se solicita cancelar.
pub struct ProfileJob {
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<Result<SampleOutcome, String>>,
}

impl Drop for ProfileJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl ProfileJob {
    pub fn start(
        path: PathBuf,
        bounds: Bounds,
        expected_size: u64,
        expected_triangles: u32,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        validate_bounds(bounds)?;
        if expected_triangles == 0 || expected_triangles as usize > MAX_TRIANGLES {
            return Err(format!(
                "La muestra acepta entre 1 y {MAX_TRIANGLES} triángulos"
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = Arc::clone(&cancelled);
        let (tx, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("lagucam-sample-profiles".into())
            .spawn(move || {
                let result =
                    calculate_profiles(&path, bounds, expected_size, expected_triangles, &token);
                // Si se canceló y se descartó el receptor, no se publica nada.
                let _ = tx.try_send(result);
                ctx.request_repaint();
            })
            .map_err(|err| format!("No se pudo iniciar el cálculo: {err}"))?;
        Ok(Self {
            cancelled,
            receiver,
        })
    }

    /// None indica cálculo en curso; Some indica resultado o fallo definitivo.
    pub fn poll(&self) -> Option<Result<SampleOutcome, String>> {
        match self.receiver.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(
                "El hilo de cálculo terminó sin devolver un resultado".into(),
            )),
        }
    }
}

fn validate_bounds(b: Bounds) -> Result<(), String> {
    if !b
        .min
        .iter()
        .chain(b.max.iter())
        .all(|value| value.is_finite())
        || b.max[0] <= b.min[0]
        || b.max[1] <= b.min[1]
        || !b.min.iter().zip(b.max).all(|(lo, hi)| *lo <= hi)
    {
        return Err("La muestra requiere límites STL válidos y extensión positiva en XY".into());
    }
    Ok(())
}

fn sample_position(bounds: Bounds, pass: usize, point: usize) -> (f64, f64) {
    let fy = 0.15 + pass as f64 * (0.70 / (PASSES - 1) as f64);
    let fx = 0.05 + point as f64 * (0.90 / (SAMPLES_PER_PASS - 1) as f64);
    (
        bounds.min[0] + (bounds.max[0] - bounds.min[0]) * fx,
        bounds.min[1] + (bounds.max[1] - bounds.min[1]) * fy,
    )
}

fn calculate_profiles(
    path: &Path,
    bounds: Bounds,
    expected_size: u64,
    expected_triangles: u32,
    cancelled: &AtomicBool,
) -> Result<SampleOutcome, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Cálculo cancelado".into());
    }
    if fs::metadata(path)
        .map_err(|err| format!("No se puede consultar el STL: {err}"))?
        .len()
        != expected_size
    {
        return Err("El STL ha cambiado de tamaño desde que se abrió en LaguCAM".into());
    }
    let (mut engine, report) = BallContactEngine::open_stl(
        path,
        ContactEngineLimits {
            max_triangles: MAX_TRIANGLES,
            max_accounted_bytes: MAX_ACCOUNTED_BYTES,
            cache_bytes: CACHE_BYTES,
        },
        cancelled,
    )
    .map_err(|err| format!("No se puede calcular la muestra: {err}"))?;
    if report.triangles != expected_triangles as usize
        || fs::metadata(path).map_err(|err| err.to_string())?.len() != expected_size
    {
        return Err("El STL ya no coincide con el que se abrió en LaguCAM".into());
    }
    let mut passes: [Vec<Option<[f64; 3]>>; PASSES] =
        std::array::from_fn(|_| Vec::with_capacity(SAMPLES_PER_PASS));
    let mut contact_count = 0;
    for (pass, row) in passes.iter_mut().enumerate() {
        for point in 0..SAMPLES_PER_PASS {
            let (x, y) = sample_position(bounds, pass, point);
            let result = engine
                .query(
                    BallContactQuery {
                        x_model: x,
                        y_model: y,
                        radius_model: BALL_RADIUS,
                    },
                    cancelled,
                )
                .map_err(|err| format!("Error en consulta de superficie: {err}"))?;
            if let Some(z) = result.tip_z_model {
                contact_count += 1;
                row.push(Some([x, y, z]));
            } else {
                row.push(None);
            }
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("Cálculo cancelado".into());
    }
    let profiles = Profiles::from_computed_passes(passes)?;
    Ok(SampleOutcome {
        profiles,
        contact_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_405_sample_locations_as_first_svg() {
        let bounds = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [100.0, 200.0, 50.0],
        };
        let (x0, y0) = sample_position(bounds, 0, 0);
        let (x1, y1) = sample_position(bounds, PASSES - 1, SAMPLES_PER_PASS - 1);
        assert_eq!((x0, y0), (5.0, 30.0));
        assert_eq!((x1, y1), (95.0, 170.0));
        assert_eq!(PASSES * SAMPLES_PER_PASS, 405);
    }

    #[test]
    fn refuses_invalid_or_xy_flat_bounds() {
        let good = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 20.0, 0.0],
        };
        assert!(validate_bounds(good).is_ok());
        let bad = Bounds {
            max: [0.0, 20.0, 0.0],
            ..good
        };
        assert!(validate_bounds(bad).is_err());
        let nan = Bounds {
            min: [f64::NAN, 0.0, 0.0],
            ..good
        };
        assert!(validate_bounds(nan).is_err());
    }

    #[test]
    fn cancellation_does_not_start_opening_the_file() {
        let cancel = AtomicBool::new(true);
        let bounds = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 20.0, 0.0],
        };
        assert!(matches!(
            calculate_profiles(
                Path::new("fichero_que_no_existe.stl"), bounds, 134, 1, &cancel,
            ),
            Err(message) if message.contains("cancelado")
        ));
    }
}

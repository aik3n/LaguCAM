//! Vista STL por bloques. No se usa esta geometría normalizada para calcular CAM.
use crate::stl::{self, Bounds, StlInfo};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32},
        mpsc::{SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

const TRIANGLES_PER_CHUNK: usize = 65_536;
const FLOATS_PER_TRIANGLE: usize = 18;

pub struct PreviewMesh {
    pub vertices: Vec<f32>,
}

pub enum LoadEvent {
    Chunk(PreviewMesh),
    Finished { info: StlInfo, time_seconds: f32 },
    Failed(String),
}

fn append_triangle(
    vertices: &mut Vec<f32>,
    triangle: [[f32; 3]; 3],
    bounds: &Bounds,
) -> Result<(), String> {
    vertices
        .try_reserve(FLOATS_PER_TRIANGLE)
        .map_err(|e| format!("Memoria insuficiente para bloque visual: {e}"))?;
    let center = bounds.center();
    let scale = 2.0 / bounds.dimensions().into_iter().fold(0.0_f64, f64::max);
    let p = triangle.map(|v| {
        glam::DVec3::from_array(std::array::from_fn(|i| {
            (f64::from(v[i]) - center[i]) * scale
        }))
    });
    let normal = (p[1] - p[0])
        .cross(p[2] - p[0])
        .normalize_or_zero()
        .as_vec3();
    for point in p {
        vertices.extend_from_slice(&point.as_vec3().to_array());
        vertices.extend_from_slice(&normal.to_array());
    }
    Ok(())
}

fn send_event(
    tx: &SyncSender<LoadEvent>,
    cancel: &AtomicBool,
    event: LoadEvent,
) -> Result<(), String> {
    let mut event = event;
    loop {
        stl::check_cancel(cancel)?;
        match tx.try_send(event) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Full(value)) => {
                event = value;
                thread::sleep(Duration::from_millis(8));
            }
            Err(TrySendError::Disconnected(_)) => {
                return Err("Se ha cerrado el receptor de importación".into());
            }
        }
    }
}

fn load_stream(
    path: &Path,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    tx: &SyncSender<LoadEvent>,
) -> Result<(StlInfo, f32), String> {
    let start = Instant::now();
    let mut chunk = Vec::new();
    let info = stl::scan(path, progress, cancel, |triangle, bounds| {
        append_triangle(&mut chunk, triangle, bounds)?;
        if chunk.len() >= TRIANGLES_PER_CHUNK * FLOATS_PER_TRIANGLE {
            send_event(
                tx,
                cancel,
                LoadEvent::Chunk(PreviewMesh {
                    vertices: std::mem::take(&mut chunk),
                }),
            )?;
        }
        Ok(())
    })?;
    if !chunk.is_empty() {
        send_event(
            tx,
            cancel,
            LoadEvent::Chunk(PreviewMesh { vertices: chunk }),
        )?;
    }
    stl::check_cancel(cancel)?;
    Ok((info, start.elapsed().as_secs_f32()))
}

/// Ejecutar desde un hilo. El canal debe ser acotado para impedir acumulación de RAM.
pub fn produce(path: &Path, progress: &AtomicU32, cancel: &AtomicBool, tx: &SyncSender<LoadEvent>) {
    let event = match load_stream(path, progress, cancel, tx) {
        Ok((info, time_seconds)) => LoadEvent::Finished { info, time_seconds },
        Err(error) => LoadEvent::Failed(error),
    };
    // Al terminar, el receptor sigue consumiendo eventos y el envío final no pierde el estado.
    let mut event = event;
    loop {
        match tx.try_send(event) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => break,
            Err(TrySendError::Full(value)) => {
                event = value;
                thread::sleep(Duration::from_millis(8));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    #[test]
    fn preserves_lower_and_vertical_faces() {
        let bounds = Bounds {
            min: [0.; 3],
            max: [1.; 3],
        };
        let mut vertices = Vec::new();
        append_triangle(
            &mut vertices,
            [[0., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            &bounds,
        )
        .unwrap();
        assert_eq!(vertices.len(), 18);
        assert_eq!(&vertices[0..3], &[-1., -1., -1.]);
        assert_eq!(&vertices[12..15], &[-1., -1., 1.]);
    }
    #[test]
    fn normalizes_extreme_finite_positions() {
        let bounds = Bounds {
            min: [-(f32::MAX as f64); 3],
            max: [f32::MAX as f64; 3],
        };
        let mut vertices = Vec::new();
        append_triangle(
            &mut vertices,
            [[-f32::MAX, 0., 0.], [f32::MAX, 0., 0.], [0., f32::MAX, 0.]],
            &bounds,
        )
        .unwrap();
        assert!(vertices.iter().all(|x| x.is_finite()));
    }
    #[test]
    fn reads_example_without_dropping_faces() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/terrain_demo.stl"
        ));
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let worker = std::thread::spawn(move || {
            produce(path, &AtomicU32::new(0), &AtomicBool::new(false), &tx);
        });
        let mut triangles = 0usize;
        let mut finished = false;
        for event in rx {
            match event {
                LoadEvent::Chunk(chunk) => triangles += chunk.vertices.len() / 18,
                LoadEvent::Finished { info, .. } => {
                    assert_eq!(info.triangles, 32_768);
                    finished = true;
                    break;
                }
                LoadEvent::Failed(error) => panic!("{error}"),
            }
        }
        worker.join().unwrap();
        assert!(finished);
        assert_eq!(triangles, 32_768);
    }
    #[test]
    fn cancel_while_channel_is_full() {
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        tx.send(LoadEvent::Chunk(PreviewMesh { vertices: vec![] }))
            .unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let other = cancel.clone();
        let worker = thread::spawn(move || {
            send_event(
                &tx,
                &other,
                LoadEvent::Chunk(PreviewMesh { vertices: vec![] }),
            )
        });
        thread::sleep(Duration::from_millis(20));
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(worker.join().unwrap().unwrap_err(), stl::CANCELLED);
    }
}

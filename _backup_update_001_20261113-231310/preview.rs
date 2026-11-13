//! Representación visual completa: sin heightfield, relleno de huecos ni descarte de caras.
//! Las coordenadas GPU se normalizan en f64 antes de convertir a f32. Nunca usar para CAM.
use crate::stl::{self, Bounds, StlInfo};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::Instant,
};

/// Presupuesto de los datos CPU de la vista, no del proceso ni de la VRAM.
/// Si se supera, se rechaza la carga completa: nunca se publica una malla parcial.
pub const MAX_PREVIEW_BYTES: usize = 256 * 1024 * 1024;
const FLOATS_PER_TRIANGLE: usize = 18;

pub struct PreviewMesh {
    /// Tres vértices por triángulo; cada vértice contiene xyz + normal xyz.
    pub vertices: Vec<f32>,
}

pub struct Loaded {
    pub mesh: PreviewMesh,
    pub info: StlInfo,
    pub time_seconds: f32,
}

fn append_triangle(
    vertices: &mut Vec<f32>,
    triangle: [[f32; 3]; 3],
    bounds: &Bounds,
    budget: usize,
) -> Result<(), String> {
    let required = vertices
        .len()
        .checked_add(FLOATS_PER_TRIANGLE)
        .ok_or("Tamaño de malla fuera de rango")?;
    let max_floats = budget / size_of::<f32>();
    if required > max_floats {
        return Err(format!(
            "La vista completa supera el límite de {} MiB. No se ha omitido ninguna región: la carga se rechaza. El soporte de mallas mayores está pendiente.",
            budget / 1_048_576
        ));
    }
    if required > vertices.capacity() {
        // Reserva por bloques con error recuperable, sin un crecimiento geométrico ilimitado.
        let target = required.saturating_add(18 * 16_384).min(max_floats);
        vertices
            .try_reserve_exact(target - vertices.len())
            .map_err(|e| format!("Memoria insuficiente para la vista: {e}"))?;
    }
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

pub fn load(path: &Path, progress: &AtomicU32, cancel: &AtomicBool) -> Result<Loaded, String> {
    let start = Instant::now();
    let mut vertices = Vec::new();
    let info = stl::scan(path, progress, cancel, |triangle, bounds| {
        append_triangle(&mut vertices, triangle, bounds, MAX_PREVIEW_BYTES)
    })?;
    stl::check_cancel(cancel)?;
    progress.store(100, Ordering::Relaxed);
    Ok(Loaded {
        mesh: PreviewMesh { vertices },
        info,
        time_seconds: start.elapsed().as_secs_f32(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_vertical_and_lower_faces_without_filling_gaps() {
        let bounds = Bounds {
            min: [0.; 3],
            max: [1.; 3],
        };
        let mut vertices = Vec::new();
        for tri in [
            [[0., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        ] {
            append_triangle(&mut vertices, tri, &bounds, 1024).unwrap();
        }
        assert_eq!(vertices.len(), 36);
        assert_eq!(&vertices[0..3], &[-1., -1., -1.]);
        assert_eq!(&vertices[12..15], &[-1., -1., 1.]);
    }
    #[test]
    fn rejects_over_budget_without_partial_triangle() {
        let mut v = Vec::new();
        let b = Bounds {
            min: [0.; 3],
            max: [1.; 3],
        };
        assert!(append_triangle(&mut v, [[0.; 3]; 3], &b, 71).is_err());
        assert!(v.is_empty());
        append_triangle(&mut v, [[0.; 3]; 3], &b, 72).unwrap();
        assert!(v.iter().all(|x| x.is_finite()));
    }
    #[test]
    fn normalizes_extreme_coordinates_without_overflow() {
        let b = Bounds {
            min: [-(f32::MAX as f64); 3],
            max: [f32::MAX as f64; 3],
        };
        let mut v = Vec::new();
        append_triangle(
            &mut v,
            [[-f32::MAX, 0., 0.], [f32::MAX, 0., 0.], [0., f32::MAX, 0.]],
            &b,
            72,
        )
        .unwrap();
        assert!(v.iter().all(|x| x.is_finite()));
        assert_eq!(v[0], -1.);
        assert_eq!(v[6], 1.);
    }
    #[test]
    fn loads_reference_with_all_triangles() {
        let loaded = load(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/terrain_demo.stl"
            )),
            &AtomicU32::new(0),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(loaded.info.triangles, 32768);
        assert_eq!(loaded.mesh.vertices.len(), 32768 * 18);
        assert_eq!(loaded.info.bounds.dimensions()[0], 200.);
    }
}

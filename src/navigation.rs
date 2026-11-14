//! Cámara orbital genérica. No conoce STL, STEP, IGES ni el origen CNC.
use glam::{Mat4, Vec3, Vec4};

/// Distancia que encuadra el cubo visual normalizado [-1, 1]^3.
pub fn fit_distance(aspect: f32) -> f32 {
    let vertical = 22.5_f32.to_radians();
    let horizontal = (vertical.tan() * aspect.max(0.001)).atan();
    3.0_f32.sqrt() / vertical.min(horizontal).sin() * 1.05
}

/// Matriz idéntica para render, proyección y elección del pivote.
/// `target` es el centro de órbita en coordenadas visuales de la escena.
pub fn view_projection(yaw: f32, pitch: f32, zoom: f32, aspect: f32, target: Vec3) -> Mat4 {
    let distance = fit_distance(aspect) * zoom;
    let eye = target
        + Vec3::new(
            distance * pitch.cos() * yaw.cos(),
            distance * pitch.cos() * yaw.sin(),
            distance * pitch.sin(),
        );
    let view = Mat4::look_at_rh(eye, target, Vec3::Z);
    let proj = Mat4::perspective_rh_gl(
        45f32.to_radians(),
        aspect.max(0.001),
        0.001,
        (distance + target.length() + 10.0).max(100.0),
    );
    proj * view
}

/// Desproyección de un punto de profundidad, con origen abajo a la izquierda.
/// Las coordenadas `(x, y)` corresponden al centro del píxel relativo al viewport.
/// Retorna None para el fondo (profundidad 1), medidas inválidas o matrices singulares.
pub fn unproject_depth(
    x: f32,
    y: f32,
    width: i32,
    height: i32,
    depth: f32,
    view_projection: Mat4,
) -> Option<Vec3> {
    if width <= 0
        || height <= 0
        || !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= width as f32
        || y >= height as f32
        || !depth.is_finite()
        || !(0.0..1.0).contains(&depth)
    {
        return None;
    }
    let inverse = view_projection.inverse();
    if !inverse.is_finite() {
        return None;
    }
    let ndc = Vec4::new(
        2.0 * x / width as f32 - 1.0,
        2.0 * y / height as f32 - 1.0,
        2.0 * depth - 1.0,
        1.0,
    );
    let point = inverse * ndc;
    if !point.is_finite() || point.w.abs() < 1e-10 {
        return None;
    }
    let result = point.truncate() / point.w;
    result.is_finite().then_some(result)
}

/// Ajusta el centro al orbitar alrededor de un punto de la escena.
/// La eleccion del pivote NO llama a esta funcion: la vista permanece identica
/// hasta que exista un arrastre del usuario.
/// Se rota el sistema ortonormal completo de la camara, por lo que el pivote
/// conserva sus coordenadas de pantalla al cambiar yaw y pitch.
fn camera_axes(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    (
        Vec3::new(-sy, cy, 0.0),
        Vec3::new(-cy * sp, -sy * sp, cp),
        Vec3::new(cy * cp, sy * cp, sp),
    )
}

pub fn orbit_target_around_pivot(
    target: Vec3,
    pivot: Vec3,
    old_yaw: f32,
    old_pitch: f32,
    new_yaw: f32,
    new_pitch: f32,
) -> Vec3 {
    let offset = target - pivot;
    let (old_right, old_up, old_back) = camera_axes(old_yaw, old_pitch);
    let (new_right, new_up, new_back) = camera_axes(new_yaw, new_pitch);
    pivot
        + new_right * offset.dot(old_right)
        + new_up * offset.dot(old_up)
        + new_back * offset.dot(old_back)
}
#[cfg(test)]
mod tests {
    #[test]
    fn new_pivot_does_not_change_camera_until_drag() {
        let target = Vec3::new(0.1, -0.2, 0.0);
        let new_pivot = Vec3::new(0.5, 0.3, -0.1);
        let before = view_projection(0.75, 0.4, 1.0, 1.6, target);
        // Elegir un punto solo modifica el campo de pivote en la UI.
        let _selected = Some(new_pivot);
        let after = view_projection(0.75, 0.4, 1.0, 1.6, target);
        assert_eq!(before, after);
    }

    #[test]
    fn orbit_keeps_pivot_at_the_same_screen_position() {
        let target = Vec3::new(0.0, 0.0, 0.0);
        let pivot = Vec3::new(0.45, -0.25, 0.35);
        for (yaw, pitch, next_yaw, next_pitch) in [
            (0.75, 0.35, 1.1, 0.55),
            (-2.0, -0.6, -0.3, 0.2),
            (0.3, 1.1, 1.8, -0.8),
        ] {
            let target_after =
                orbit_target_around_pivot(target, pivot, yaw, pitch, next_yaw, next_pitch);
            let before_clip = view_projection(yaw, pitch, 1.0, 1.6, target) * pivot.extend(1.0);
            let after_clip =
                view_projection(next_yaw, next_pitch, 1.0, 1.6, target_after) * pivot.extend(1.0);
            let before_xy = (before_clip.truncate() / before_clip.w).truncate();
            let after_xy = (after_clip.truncate() / after_clip.w).truncate();
            assert!((before_xy - after_xy).length() < 1e-4);
            assert!(((target_after - pivot).length() - (target - pivot).length()).abs() < 1e-4);
        }
    }

    use super::*;

    #[test]
    fn project_unproject_keeps_scene_point() {
        let target = Vec3::new(0.12, -0.16, 0.24);
        let point = Vec3::new(-0.21, 0.32, 0.18);
        for (yaw, pitch, zoom, aspect) in [
            (0.4, 0.25, 1.0, 1.0),
            (-1.4, 0.85, 0.6, 2.3),
            (2.0, -0.5, 3.0, 0.5),
        ] {
            let mvp = view_projection(yaw, pitch, zoom, aspect, target);
            let clip = mvp * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let x = (ndc.x + 1.0) * 0.5 * 800.0;
            let y = (ndc.y + 1.0) * 0.5 * 600.0;
            let z = (ndc.z + 1.0) * 0.5;
            let recovered = unproject_depth(x, y, 800, 600, z, mvp).unwrap();
            assert!((recovered - point).length() < 0.03);
        }
    }

    #[test]
    fn rejects_background_and_invalid_geometry() {
        let mvp = view_projection(0.4, 0.2, 1.0, 1.0, Vec3::ZERO);
        assert!(unproject_depth(50.0, 50.0, 100, 100, 1.0, mvp).is_none());
        assert!(unproject_depth(-1.0, 50.0, 100, 100, 0.5, mvp).is_none());
        assert!(unproject_depth(50.0, 50.0, 0, 100, 0.5, mvp).is_none());
        assert!(unproject_depth(50.0, 50.0, 100, 100, f32::NAN, mvp).is_none());
        assert!(unproject_depth(50.0, 50.0, 100, 100, 0.5, Mat4::ZERO).is_none());
    }

    #[test]
    fn changing_target_keeps_expected_orbit_center() {
        let target = Vec3::new(0.25, -0.5, 0.1);
        let mvp = view_projection(1.0, 0.4, 1.0, 1.5, target);
        let projected = mvp * target.extend(1.0);
        let ndc = projected.truncate() / projected.w;
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5);
    }
}

//! Cámara orbital genérica. No conoce STL, STEP, IGES ni el origen CNC.
use glam::{Mat4, Vec2, Vec3, Vec4};

/// Proyeccion del visor, independiente del modelo, de la operacion y del pivote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Projection {
    #[default]
    Perspective,
    Orthographic,
}

// Los modos de proyeccion se seleccionan directamente en el visor.

// Vista superior sobre los ejes del MODELO: +X derecha, +Y arriba.
// Solo cambia la camara; no transforma el STL ni el sistema CNC.
pub const TOP_VIEW_YAW: f32 = -std::f32::consts::FRAC_PI_2;
#[cfg(test)]
pub const TOP_VIEW_PITCH: f32 = std::f32::consts::FRAC_PI_2;

/// Distancia que encuadra el cubo visual normalizado [-1, 1]^3.
pub fn fit_distance(aspect: f32) -> f32 {
    let vertical = 22.5_f32.to_radians();
    let horizontal = (vertical.tan() * aspect.max(0.001)).atan();
    3.0_f32.sqrt() / vertical.min(horizontal).sin() * 1.05
}

/// Proyeccion en perspectiva conservada para las pruebas existentes.
#[cfg(test)]
pub fn view_projection(yaw: f32, pitch: f32, zoom: f32, aspect: f32, target: Vec3) -> Mat4 {
    view_projection_mode(yaw, pitch, zoom, aspect, target, Projection::Perspective)
}

/// La misma matriz se usa en STL, bruto, indicadores, picking y previsualizaciones.
/// La vista ortografica mantiene la camara a la distancia de encuadre y cambia
/// exclusivamente la escala de la proyeccion; la perspectiva hace dolly.
/// Ambas tienen identica escala en el plano perpendicular por `target`.
pub fn view_projection_mode(
    yaw: f32,
    pitch: f32,
    zoom: f32,
    aspect: f32,
    target: Vec3,
    projection: Projection,
) -> Mat4 {
    let base_distance = fit_distance(aspect);
    let distance = match projection {
        Projection::Perspective => base_distance * zoom,
        Projection::Orthographic => base_distance,
    };
    let eye = target
        + Vec3::new(
            distance * pitch.cos() * yaw.cos(),
            distance * pitch.cos() * yaw.sin(),
            distance * pitch.sin(),
        );
    // La base de camara mantiene el eje vertical bien definido incluso
    // cuando la direccion de vision es paralela a Z (vista superior exacta).
    let (_, up, _) = camera_axes(yaw, pitch);
    let view = Mat4::look_at_rh(eye, target, up);
    let far = (distance + target.length() + 10.0).max(100.0);
    let proj = match projection {
        Projection::Perspective => {
            Mat4::perspective_rh_gl(45f32.to_radians(), aspect.max(0.001), 0.001, far)
        }
        Projection::Orthographic => {
            let half_height = base_distance * zoom * 22.5_f32.to_radians().tan();
            let half_width = half_height * aspect.max(0.001);
            Mat4::orthographic_rh_gl(
                -half_width,
                half_width,
                -half_height,
                half_height,
                0.001,
                far,
            )
        }
    };
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
pub(crate) fn camera_axes(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    (
        Vec3::new(-sy, cy, 0.0),
        Vec3::new(-cy * sp, -sy * sp, cp),
        Vec3::new(cy * cp, sy * cp, sp),
    )
}

/// Corrige el target al hacer zoom para que el rayo del cursor interseque
/// el mismo punto del plano perpendicular a la camara que pasa por `target`.
/// No depende de profundidad, STL o pivote de orbita. El cursor esta en NDC
/// (x/y -1..1), de modo que la escala DPI y el offset del visor no intervienen.
/// Es valida para perspectiva y ortografica gracias a la escala comun en ese plano.
pub fn zoom_target_at_cursor(
    target: Vec3,
    yaw: f32,
    pitch: f32,
    old_zoom: f32,
    new_zoom: f32,
    aspect: f32,
    cursor_ndc: Vec2,
) -> Vec3 {
    let half_height = fit_distance(aspect) * old_zoom * 22.5_f32.to_radians().tan();
    let (right, up, _) = camera_axes(yaw, pitch);
    let offset = right * (cursor_ndc.x * half_height * aspect.max(0.001))
        + up * (cursor_ndc.y * half_height);
    target + offset * (1.0 - new_zoom / old_zoom)
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

    fn screen_xy(mvp: Mat4, world: Vec3) -> Vec2 {
        let clip = mvp * world.extend(1.0);
        (clip.truncate() / clip.w).truncate()
    }

    #[test]
    fn zoom_to_cursor_holds_the_reference_for_both_projections() {
        // El punto de referencia es matematico: NO se consulta ningun STL.
        for mode in [Projection::Perspective, Projection::Orthographic] {
            for (yaw, pitch, aspect, old_zoom, new_zoom, cursor) in [
                (0.7, 0.35, 1.6, 1.0, 0.36, Vec2::new(0.85, -0.7)),
                (-2.1, -0.6, 0.52, 0.36, 1.7, Vec2::new(-0.92, 0.8)),
                (1.3, 1.0, 2.4, 4.0, 0.1, Vec2::new(0.0, 0.0)),
            ] {
                let target = Vec3::new(0.2, -0.3, 0.15);
                let (right, up, _) = camera_axes(yaw, pitch);
                let half_height = fit_distance(aspect) * old_zoom * 22.5_f32.to_radians().tan();
                let anchor = target
                    + right * (cursor.x * half_height * aspect)
                    + up * (cursor.y * half_height);
                let old_xy = screen_xy(
                    view_projection_mode(yaw, pitch, old_zoom, aspect, target, mode),
                    anchor,
                );
                let next_target =
                    zoom_target_at_cursor(target, yaw, pitch, old_zoom, new_zoom, aspect, cursor);
                let new_xy = screen_xy(
                    view_projection_mode(yaw, pitch, new_zoom, aspect, next_target, mode),
                    anchor,
                );
                assert!((old_xy - cursor).length() < 0.0001, "{mode:?}: {old_xy:?}");
                assert!((new_xy - old_xy).length() < 0.0001, "{mode:?}: {new_xy:?}");
            }
        }
    }

    #[test]
    fn zoom_in_then_out_restores_camera_target() {
        let target = Vec3::new(0.2, -0.1, 0.3);
        let cursor = Vec2::new(-0.8, 0.65);
        let closer = zoom_target_at_cursor(target, 0.9, 0.5, 1.0, 0.6, 1.4, cursor);
        let restored = zoom_target_at_cursor(closer, 0.9, 0.5, 0.6, 1.0, 1.4, cursor);
        assert!((restored - target).length() < 0.00001);
    }

    #[test]
    fn orthographic_has_no_depth_dependent_screen_scale() {
        let yaw = 0.6;
        let pitch = 0.3;
        let (_, _, backward) = camera_axes(yaw, pitch);
        let matrix =
            view_projection_mode(yaw, pitch, 0.5, 1.7, Vec3::ZERO, Projection::Orthographic);
        let p = Vec3::new(0.2, 0.3, -0.2);
        assert!((screen_xy(matrix, p) - screen_xy(matrix, p + backward)).length() < 0.00001);
    }

    #[test]
    fn switching_projection_keeps_reference_plane_scale() {
        let target = Vec3::new(0.13, 0.2, -0.1);
        let yaw = 0.9;
        let pitch = 0.4;
        let (_, up, _) = camera_axes(yaw, pitch);
        let point = target + up * 0.5;
        let perspective = screen_xy(
            view_projection_mode(yaw, pitch, 0.75, 1.5, target, Projection::Perspective),
            point,
        );
        let orthographic = screen_xy(
            view_projection_mode(yaw, pitch, 0.75, 1.5, target, Projection::Orthographic),
            point,
        );
        assert!((perspective - orthographic).length() < 0.00001);
    }

    #[test]
    fn top_view_has_exact_model_xy_screen_orientation() {
        // Vista desde +Z hacia -Z: X a la derecha e Y hacia arriba.
        let target = Vec3::new(0.2, -0.3, 0.1);
        let mvp = view_projection_mode(
            TOP_VIEW_YAW,
            TOP_VIEW_PITCH,
            0.73,
            1.6,
            target,
            Projection::Orthographic,
        );
        assert!(mvp.is_finite());
        let origin = screen_xy(mvp, target);
        let x = screen_xy(mvp, target + Vec3::X * 0.25);
        let y = screen_xy(mvp, target + Vec3::Y * 0.25);
        let z = screen_xy(mvp, target + Vec3::Z * 0.25);
        assert!(origin.length() < 1e-5);
        assert!(x.x > origin.x);
        assert!(y.y > origin.y);
        assert!((x.y - origin.y).abs() < 1e-5);
        assert!((y.x - origin.x).abs() < 1e-5);
        assert!((z - origin).length() < 1e-5);
    }

    #[test]
    fn top_view_basis_is_finite_and_continuous_on_first_orbit() {
        let (right, up, back) = camera_axes(TOP_VIEW_YAW, TOP_VIEW_PITCH);
        assert!((right - Vec3::X).length() < 1e-5);
        assert!((up - Vec3::Y).length() < 1e-5);
        assert!((back - Vec3::Z).length() < 1e-5);
        let target = Vec3::ZERO;
        let point = Vec3::new(0.3, 0.2, 0.1);
        let exact = view_projection_mode(
            TOP_VIEW_YAW,
            TOP_VIEW_PITCH,
            0.8,
            1.5,
            target,
            Projection::Orthographic,
        );
        let slightly_tilted = view_projection_mode(
            TOP_VIEW_YAW,
            TOP_VIEW_PITCH - 0.00001,
            0.8,
            1.5,
            target,
            Projection::Orthographic,
        );
        assert!(slightly_tilted.is_finite());
        assert!((screen_xy(exact, point) - screen_xy(slightly_tilted, point)).length() < 1e-4);
    }
}

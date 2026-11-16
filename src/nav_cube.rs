//! Cubo de orientacion del visor (solo UI, no participa en CAM ni en el render STL).
//! Las caras eligen vistas axiales y los vertices una vista isometrica.
use crate::navigation;
use eframe::egui;
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct SnapTarget {
    pub yaw: f32,
    pub pitch: f32,
    pub axial: bool,
}

impl SnapTarget {
    fn from_direction(dir: Vec3, axial: bool) -> Self {
        let horizontal = dir.x.hypot(dir.y);
        let yaw = if horizontal < 0.0001 {
            navigation::TOP_VIEW_YAW
        } else {
            dir.y.atan2(dir.x)
        };
        let pitch = dir.z.atan2(horizontal);
        Self { yaw, pitch, axial }
    }
}

pub fn viewport_rect(viewport: egui::Rect) -> Option<egui::Rect> {
    (viewport.width() >= 180.0 && viewport.height() >= 178.0).then(|| {
        egui::Rect::from_min_size(
            egui::pos2(viewport.right() - 126.0, viewport.top() + 8.0),
            egui::vec2(114.0, 114.0),
        )
    })
}

#[derive(Clone, Copy)]
struct Face {
    normal: Vec3,
    label: &'static str,
    vertices: [Vec3; 4],
    color: egui::Color32,
}

fn faces() -> [Face; 6] {
    let v = Vec3::new;
    [
        Face {
            normal: Vec3::Z,
            label: "+Z",
            vertices: [
                v(-1., -1., 1.),
                v(1., -1., 1.),
                v(1., 1., 1.),
                v(-1., 1., 1.),
            ],
            color: egui::Color32::from_rgb(141, 161, 185),
        },
        Face {
            normal: -Vec3::Z,
            label: "-Z",
            vertices: [
                v(-1., -1., -1.),
                v(-1., 1., -1.),
                v(1., 1., -1.),
                v(1., -1., -1.),
            ],
            color: egui::Color32::from_rgb(122, 143, 164),
        },
        Face {
            normal: Vec3::X,
            label: "+X",
            vertices: [
                v(1., -1., -1.),
                v(1., 1., -1.),
                v(1., 1., 1.),
                v(1., -1., 1.),
            ],
            color: egui::Color32::from_rgb(95, 123, 151),
        },
        Face {
            normal: -Vec3::X,
            label: "-X",
            vertices: [
                v(-1., -1., -1.),
                v(-1., -1., 1.),
                v(-1., 1., 1.),
                v(-1., 1., -1.),
            ],
            color: egui::Color32::from_rgb(95, 123, 151),
        },
        Face {
            normal: Vec3::Y,
            label: "+Y",
            vertices: [
                v(-1., 1., -1.),
                v(-1., 1., 1.),
                v(1., 1., 1.),
                v(1., 1., -1.),
            ],
            color: egui::Color32::from_rgb(110, 138, 163),
        },
        Face {
            normal: -Vec3::Y,
            label: "-Y",
            vertices: [
                v(-1., -1., -1.),
                v(1., -1., -1.),
                v(1., -1., 1.),
                v(-1., -1., 1.),
            ],
            color: egui::Color32::from_rgb(110, 138, 163),
        },
    ]
}

fn project(point: Vec3, right: Vec3, up: Vec3, center: egui::Pos2) -> egui::Pos2 {
    let scale = 25.5;
    center + egui::vec2(point.dot(right) * scale, -point.dot(up) * scale)
}

fn inside_convex(poly: &[egui::Pos2; 4], p: egui::Pos2) -> bool {
    let mut positive = false;
    let mut negative = false;
    for i in 0..4 {
        let a = poly[i];
        let b = poly[(i + 1) % 4];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        positive |= cross > 0.001;
        negative |= cross < -0.001;
    }
    !(positive && negative)
}

/// Pinta usando el egui Painter. Sin una segunda escena OpenGL o lecturas GPU.
/// Usa el clic ya adquirido por el viewport; evita widgets superpuestos.
pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    yaw: f32,
    pitch: f32,
    pointer: Option<egui::Pos2>,
    clicked: bool,
) -> Option<SnapTarget> {
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(8),
        egui::Color32::from_rgba_unmultiplied(25, 31, 40, 225),
    );
    let (right, up, back) = navigation::camera_axes(yaw, pitch);
    let all = faces();
    let mut visible: Vec<(usize, [egui::Pos2; 4], f32)> = Vec::with_capacity(3);
    for (index, face) in all.iter().enumerate() {
        let toward_camera = face.normal.dot(back);
        if toward_camera > 0.00001 {
            let points = face.vertices.map(|v| project(v, right, up, rect.center()));
            visible.push((index, points, toward_camera));
        }
    }
    visible.sort_by(|a, b| a.2.total_cmp(&b.2));

    // Preferir vertices: son las ocho posiciones isometricas accesibles.
    let mut corner_hit: Option<(Vec3, f32)> = None;
    if let Some(p) = pointer {
        for (face_index, _, _) in &visible {
            for v in all[*face_index].vertices {
                let dist = p.distance(project(v, right, up, rect.center()));
                if dist <= 8.5 && corner_hit.is_none_or(|(_, prev)| dist < prev) {
                    corner_hit = Some((v, dist));
                }
            }
        }
    }
    let face_hit = pointer.and_then(|p| {
        visible
            .iter()
            .rev()
            .find(|(_, poly, _)| inside_convex(poly, p))
            .map(|(index, _, _)| *index)
    });
    for (index, poly, _) in &visible {
        let face = &all[*index];
        let hovered = corner_hit.is_none() && face_hit == Some(*index);
        let fill = if hovered {
            egui::Color32::from_rgb(173, 193, 212)
        } else {
            face.color
        };
        painter.add(egui::Shape::convex_polygon(
            poly.to_vec(),
            fill,
            egui::Stroke::new(1.4, egui::Color32::from_rgb(218, 229, 239)),
        ));
        let label_pos = project(face.normal, right, up, rect.center());
        if face.normal.dot(back) >= 0.21 {
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                face.label,
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(23, 35, 48),
            );
        }
    }
    // Puntos discretos en los vertices visibles: hacen reconocible la accion isometrica.
    let mut drawn = Vec::<egui::Pos2>::new();
    for (index, _, _) in &visible {
        for corner in all[*index].vertices {
            let pos = project(corner, right, up, rect.center());
            if drawn.iter().any(|other| other.distance(pos) < 0.75) {
                continue;
            }
            drawn.push(pos);
            let highlight = corner_hit.is_some_and(|(v, _)| (v - corner).length_squared() < 0.1);
            painter.circle_filled(
                pos,
                if highlight { 5.0 } else { 2.8 },
                if highlight {
                    egui::Color32::from_rgb(255, 218, 104)
                } else {
                    egui::Color32::from_rgb(226, 233, 240)
                },
            );
        }
    }
    if corner_hit.is_some() || face_hit.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !clicked {
        return None;
    }
    if let Some((corner, _)) = corner_hit {
        return Some(SnapTarget::from_direction(corner.normalize(), false));
    }
    face_hit.map(|index| SnapTarget::from_direction(all[index].normal, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn top_face_is_exact_and_preserves_xy_orientation() {
        let view = SnapTarget::from_direction(Vec3::Z, true);
        assert!((view.yaw - navigation::TOP_VIEW_YAW).abs() < 1e-6);
        assert!((view.pitch - navigation::TOP_VIEW_PITCH).abs() < 1e-6);
        let (right, up, back) = navigation::camera_axes(view.yaw, view.pitch);
        assert!((right - Vec3::X).length() < 1e-5);
        assert!((up - Vec3::Y).length() < 1e-5);
        assert!((back - Vec3::Z).length() < 1e-5);
    }
    #[test]
    fn all_six_faces_and_eight_corners_produce_finite_cameras() {
        for face in faces() {
            let snap = SnapTarget::from_direction(face.normal, true);
            let vp = navigation::view_projection_mode(
                snap.yaw,
                snap.pitch,
                1.0,
                1.4,
                Vec3::ZERO,
                navigation::Projection::Orthographic,
            );
            assert!(vp.is_finite(), "{}", face.label);
        }
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let snap = SnapTarget::from_direction(Vec3::new(x, y, z), false);
                    assert!(!snap.axial);
                    assert!(snap.yaw.is_finite() && snap.pitch.is_finite());
                }
            }
        }
    }
    #[test]
    fn polygon_hit_test_respects_visible_face() {
        let poly = [
            egui::pos2(0., 0.),
            egui::pos2(10., 0.),
            egui::pos2(10., 10.),
            egui::pos2(0., 10.),
        ];
        assert!(inside_convex(&poly, egui::pos2(5., 5.)));
        assert!(!inside_convex(&poly, egui::pos2(12., 5.)));
    }
}

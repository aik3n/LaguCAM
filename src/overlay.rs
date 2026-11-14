//! Indicador CNC y puntos de selección (2D proyectados desde coordenadas f64).
//! Los marcadores no alteran el STL ni los datos CAM.
use crate::project::{
    Project,
    origin::{OriginPoint, OriginReference},
    selection::Selection,
};
use eframe::egui;
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: Vec3,
}

#[derive(Clone, Copy)]
struct Marker {
    reference: OriginReference,
    point: OriginPoint,
    pos: egui::Pos2,
    depth: f32,
}

pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    project: &mut Project,
    camera: Camera,
    clicked: bool,
    pointer: Option<egui::Pos2>,
) {
    let (Some(model), Some(stock)) = (&project.model, &project.stock) else {
        return;
    };
    let aspect = rect.width() / rect.height().max(1.0);
    let distance = crate::render::fit_distance(aspect) * camera.zoom;
    let eye = camera.pan
        + Vec3::new(
            distance * camera.pitch.cos() * camera.yaw.cos(),
            distance * camera.pitch.cos() * camera.yaw.sin(),
            distance * camera.pitch.sin(),
        );
    let view = Mat4::look_at_rh(eye, camera.pan, Vec3::Z);
    let proj = Mat4::perspective_rh_gl(
        45f32.to_radians(),
        aspect.max(0.001),
        0.001,
        (distance + camera.pan.length() + 10.0).max(100.0),
    );
    let mvp = proj * view;
    let screen = |p: [f64; 3]| -> Option<(egui::Pos2, f32)> {
        let pos = model.visual_point(p)?;
        let h = mvp * Vec4::new(pos.x, pos.y, pos.z, 1.0);
        if h.w <= 0.0 || !h.is_finite() {
            return None;
        }
        let x = h.x / h.w;
        let y = h.y / h.w;
        let z = h.z / h.w;
        if !x.is_finite() || !y.is_finite() || !(-1.0..=1.0).contains(&z) {
            return None;
        }
        Some((
            egui::pos2(
                rect.center().x + x * rect.width() * 0.5,
                rect.center().y - y * rect.height() * 0.5,
            ),
            z,
        ))
    };

    if project.origin.visible
        && let Ok(origin) = project.origin.model_position(stock, &model.bounds)
    {
        let axis_len = stock.size.into_iter().fold(0.0_f64, f64::max) * 0.15;
        let colors = [
            egui::Color32::RED,
            egui::Color32::GREEN,
            egui::Color32::LIGHT_BLUE,
        ];
        for (axis, color) in colors.into_iter().enumerate() {
            let mut axis_delta = [0.0_f64; 3];
            axis_delta[axis] = axis_len;
            let Ok(target) = project
                .origin
                .work_to_model(stock, &model.bounds, axis_delta)
            else {
                continue;
            };
            if let (Some((a, _)), Some((b, _))) = (screen(origin), screen(target)) {
                ui.painter()
                    .line_segment([a, b], egui::Stroke::new(3.0, color));
                ui.painter().text(
                    b,
                    egui::Align2::CENTER_CENTER,
                    ["X", "Y", "Z"][axis],
                    egui::FontId::proportional(14.0),
                    color,
                );
            }
        }
    }

    // Los puntos de selección solo existen al configurar el origen CNC.
    if project.selection != Selection::Origin {
        return;
    }
    let mut markers = Vec::with_capacity(20);
    for reference in OriginReference::ALL {
        for point in OriginPoint::ALL {
            let Ok(world) = project
                .origin
                .anchor_position(stock, &model.bounds, reference, point)
            else {
                continue;
            };
            if let Some((pos, depth)) = screen(world)
                && rect.contains(pos)
            {
                markers.push(Marker {
                    reference,
                    point,
                    pos,
                    depth,
                });
            }
        }
    }

    // Cuando coinciden dos puntos, se prioriza la referencia elegida en el panel.
    // Entre puntos de la misma referencia, se prioriza el más próximo a cámara.
    let hovered = pointer.and_then(|pointer| {
        let mut best: Option<(usize, f32)> = None;
        for (index, marker) in markers.iter().enumerate() {
            let distance_sq = marker.pos.distance_sq(pointer);
            if distance_sq > 100.0 {
                continue;
            }
            let replace = match best {
                None => true,
                Some((previous, old_distance_sq)) => {
                    if distance_sq < old_distance_sq - 0.25 {
                        true
                    } else if (distance_sq - old_distance_sq).abs() <= 0.25 {
                        let current_preferred = marker.reference == project.origin.reference;
                        let old_preferred = markers[previous].reference == project.origin.reference;
                        (current_preferred && !old_preferred)
                            || (current_preferred == old_preferred
                                && marker.depth < markers[previous].depth)
                    } else {
                        false
                    }
                }
            };
            if replace {
                best = Some((index, distance_sq));
            }
        }
        best.map(|(index, _)| index)
    });

    if let Some(index) = hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if clicked {
            let marker = markers[index];
            project.origin.reference = marker.reference;
            project.origin.point = marker.point;
            project.origin.offset = [0.0; 3];
        }
    }

    let selected = (project.origin.reference, project.origin.point);
    for (index, marker) in markers.iter().enumerate() {
        let is_hovered = hovered == Some(index);
        let is_selected = selected == (marker.reference, marker.point);
        let radius = if is_hovered { 6.0 } else { 4.0 };
        let color = egui::Color32::from_rgb(240, 55, 55);
        ui.painter().circle_filled(marker.pos, radius, color);
        if is_selected {
            ui.painter().circle_stroke(
                marker.pos,
                radius + 2.0,
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }
    }
    if let Some(index) = hovered {
        let marker = markers[index];
        ui.painter().text(
            marker.pos + egui::vec2(12.0, -11.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{} · {}", marker.reference.label(), marker.point.label()),
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::ModelReference;
    use crate::stl::Bounds;

    #[test]
    fn normalized_coordinates_are_only_a_projection() {
        let model = ModelReference {
            name: "m".into(),
            triangles: 1,
            size_bytes: 0,
            bounds: Bounds {
                min: [50.0, 100.0, 150.0],
                max: [70.0, 120.0, 170.0],
            },
        };
        assert_eq!(
            model.visual_point([50.0, 100.0, 150.0]).unwrap(),
            Vec3::splat(-1.0)
        );
        assert_eq!(model.bounds.max, [70.0, 120.0, 170.0]);
    }
}

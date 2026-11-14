//! 3D-projected construction overlays; no changes to OpenGL mesh uploads.
//! Lines are drawn over the mesh (not depth-tested).
use crate::project::Project;
use eframe::egui;
use glam::{Mat4, Vec3, Vec4};

pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    project: &Project,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan: Vec3,
) {
    let (Some(model), Some(stock)) = (&project.model, &project.stock) else {
        return;
    };
    let aspect = rect.width() / rect.height().max(1.0);
    let distance = crate::render::fit_distance(aspect) * zoom;
    let eye = pan
        + Vec3::new(
            distance * pitch.cos() * yaw.cos(),
            distance * pitch.cos() * yaw.sin(),
            distance * pitch.sin(),
        );
    let view = Mat4::look_at_rh(eye, pan, Vec3::Z);
    let proj = Mat4::perspective_rh_gl(
        45f32.to_radians(),
        aspect.max(0.001),
        0.001,
        (distance + pan.length() + 10.0).max(100.0),
    );
    let mvp = proj * view;
    let screen = |p: [f64; 3]| -> Option<egui::Pos2> {
        let pos = model.visual_point(p)?;
        let h = mvp * Vec4::new(pos.x, pos.y, pos.z, 1.0);
        if h.w <= 0.0 || !h.is_finite() {
            return None;
        }
        let x = h.x / h.w;
        let y = h.y / h.w;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some(egui::pos2(
            rect.center().x + x * rect.width() * 0.5,
            rect.center().y - y * rect.height() * 0.5,
        ))
    };
    if stock.visible
        && let Some(max) = stock.max()
    {
        let min = stock.min;
        let corners = [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [min[0], max[1], max[2]],
            [max[0], max[1], max[2]],
        ];
        let edges = [
            (0, 1),
            (0, 2),
            (1, 3),
            (2, 3),
            (4, 5),
            (4, 6),
            (5, 7),
            (6, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let stroke = egui::Stroke::new(1.8, egui::Color32::from_rgb(245, 184, 62));
        for (a, b) in edges {
            if let (Some(a), Some(b)) = (screen(corners[a]), screen(corners[b])) {
                ui.painter().line_segment([a, b], stroke);
            }
        }
    }
    if project.origin.visible
        && let Ok(origin) = project.origin.model_position(stock)
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
            let Ok(target) = project.origin.work_to_model(stock, axis_delta) else {
                continue;
            };
            if let (Some(a), Some(b)) = (screen(origin), screen(target)) {
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

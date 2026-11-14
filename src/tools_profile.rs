//! Dibujo técnico 2D de la sección axial calculada a partir del perfil real f64.
//! La representación no forma parte del modelo CAM.
use crate::tools::model::{Tool, ToolGeometry, ToolKind};
use eframe::egui;

/// Lienzo permanente. Sin valores válidos se muestra un esquema *sin escala*,
/// nunca una geometría CAM ni cotas supuestas.
pub fn draw_empty(ui: &mut egui::Ui) {
    draw_placeholder(ui, None, "Selecciona una herramienta o pulsa Nueva.");
}

fn draw_placeholder(ui: &mut egui::Ui, kind: Option<ToolKind>, message: &str) {
    let width = ui.available_width().max(250.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 400.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(25, 30, 36));
    let center_x = rect.center().x;
    let top = rect.top() + 92.0;
    let shoulder = rect.top() + 198.0;
    let bottom = rect.bottom() - 70.0;
    let guide = egui::Color32::from_gray(108);
    for i in 0..19 {
        if i % 2 == 0 {
            let y0 = rect.top() + 48.0 + i as f32 * 17.0;
            painter.line_segment(
                [egui::pos2(center_x, y0), egui::pos2(center_x, y0 + 11.0)],
                egui::Stroke::new(1.0, guide),
            );
        }
    }
    painter.line_segment(
        [
            egui::pos2(center_x - 65.0, bottom),
            egui::pos2(center_x + 65.0, bottom),
        ],
        egui::Stroke::new(0.8, guide),
    );
    if let Some(kind) = kind {
        // Dibujo tipológico de la familia seleccionada, deliberadamente sin medidas.
        // Solo la función ToolGeometry::profile construye el perfil dimensional real.
        let line = egui::Stroke::new(1.8, egui::Color32::from_rgb(95, 185, 221));
        for sign in [-1.0_f32, 1.0] {
            let p = |radius: f32, y: f32| egui::pos2(center_x + sign * radius, y);
            let mut outline = vec![p(23.0, top), p(23.0, shoulder), p(43.0, shoulder)];
            match kind {
                ToolKind::Flat => outline.push(p(43.0, bottom)),
                ToolKind::VBit => outline.push(p(0.0, bottom)),
                ToolKind::ConicalFlat => outline.push(p(12.0, bottom)),
                ToolKind::BallNose => {
                    outline.push(p(43.0, bottom - 43.0));
                    for i in (0..=16).rev() {
                        let t = std::f32::consts::FRAC_PI_2 * i as f32 / 16.0;
                        outline.push(p(43.0 * t.sin(), bottom - 43.0 * (1.0 - t.cos())));
                    }
                }
                ToolKind::ConicalBall => {
                    const R: f32 = 20.0;
                    let tangent_angle = std::f32::consts::FRAC_PI_4;
                    outline.push(p(
                        R * tangent_angle.sin(),
                        bottom - R * (1.0 - tangent_angle.cos()),
                    ));
                    for i in (0..=12).rev() {
                        let t = tangent_angle * i as f32 / 12.0;
                        outline.push(p(R * t.sin(), bottom - R * (1.0 - t.cos())));
                    }
                }
            }
            painter.add(egui::Shape::line(outline, line));
        }
        if matches!(kind, ToolKind::Flat | ToolKind::ConicalFlat) {
            let front_radius = if kind == ToolKind::Flat { 43.0 } else { 12.0 };
            painter.line_segment(
                [
                    egui::pos2(center_x - front_radius, bottom),
                    egui::pos2(center_x + front_radius, bottom),
                ],
                line,
            );
        }
    }
    let font = egui::FontId::monospace(12.0);
    painter.text(
        rect.left_top() + egui::vec2(12.0, 12.0),
        egui::Align2::LEFT_TOP,
        kind.map_or("Sin herramienta seleccionada", ToolKind::label),
        font.clone(),
        egui::Color32::WHITE,
    );
    painter.text(
        rect.left_top() + egui::vec2(12.0, 29.0),
        egui::Align2::LEFT_TOP,
        "ESQUEMA SIN ESCALA · Ø — · L —",
        font.clone(),
        egui::Color32::LIGHT_GRAY,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(12.0, -12.0),
        egui::Align2::LEFT_BOTTOM,
        message,
        font,
        egui::Color32::YELLOW,
    );
    ui.small("Completa la geometría para mostrar el perfil real con cotas.");
}

pub fn draw(ui: &mut egui::Ui, tool: &Tool) {
    let points = match tool.geometry.profile(tool.cutting_length_mm) {
        Ok(points) => points,
        Err(error) => {
            draw_placeholder(ui, Some(tool.geometry.kind()), &error);

            return;
        }
    };
    if !tool.total_length_mm.is_finite()
        || tool.total_length_mm < tool.cutting_length_mm
        || !tool.shank_diameter_mm.is_finite()
        || tool.shank_diameter_mm <= 0.0
    {
        draw_placeholder(
            ui,
            Some(tool.geometry.kind()),
            "Longitud total o mango inválidos",
        );
        return;
    }
    let width = ui.available_width().max(250.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 400.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(25, 30, 36));
    let center_x = rect.center().x;
    let bottom = rect.bottom() - 27.0;
    let largest_radius = tool.geometry.diameter_mm().max(tool.shank_diameter_mm) * 0.5;
    let horizontal_scale = f64::from((rect.width() - 115.0).max(80.0)) / (largest_radius * 2.0);
    let vertical_scale = f64::from(rect.height() - 75.0) / tool.total_length_mm;
    let scale = horizontal_scale.min(vertical_scale);
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let to_pos = |radius_mm: f64, z_mm: f64| -> egui::Pos2 {
        egui::pos2(
            center_x + (radius_mm * scale) as f32,
            bottom - (z_mm * scale) as f32,
        )
    };
    let cutter = egui::Color32::from_rgb(95, 185, 221);
    let shaft = egui::Color32::from_rgb(135, 146, 156);
    let stroke = egui::Stroke::new(1.7, egui::Color32::WHITE);
    let right: Vec<_> = points.iter().map(|p| to_pos(p.radius_mm, p.z_mm)).collect();
    let left: Vec<_> = points
        .iter()
        .map(|p| to_pos(-p.radius_mm, p.z_mm))
        .collect();
    for i in 1..points.len() {
        let quad = [left[i - 1], right[i - 1], right[i], left[i]];
        painter.add(egui::Shape::convex_polygon(
            quad.to_vec(),
            cutter,
            egui::Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(right.clone(), stroke));
    painter.add(egui::Shape::line(left.clone(), stroke));
    if let (Some(a), Some(b)) = (left.first(), right.first()) {
        painter.line_segment([*a, *b], stroke);
    }
    let end_cut = to_pos(0.0, tool.cutting_length_mm);
    let end_free = to_pos(0.0, tool.total_length_mm);
    let cutting_radius = tool.geometry.diameter_mm() * 0.5;
    let shank_radius = tool.shank_diameter_mm * 0.5;
    let shank_bottom_left = to_pos(-shank_radius, tool.cutting_length_mm);
    let shank_bottom_right = to_pos(shank_radius, tool.cutting_length_mm);
    let shank_top_left = to_pos(-shank_radius, tool.total_length_mm);
    let shank_top_right = to_pos(shank_radius, tool.total_length_mm);
    if tool.total_length_mm > tool.cutting_length_mm {
        painter.rect_filled(
            egui::Rect::from_two_pos(shank_bottom_left, shank_top_right),
            0.0,
            shaft,
        );
        painter.line_segment([shank_bottom_left, shank_top_left], stroke);
        painter.line_segment([shank_bottom_right, shank_top_right], stroke);
        painter.line_segment([shank_top_left, shank_top_right], stroke);
    }
    painter.line_segment(
        [
            to_pos(-cutting_radius, tool.cutting_length_mm),
            shank_bottom_left,
        ],
        stroke,
    );
    painter.line_segment(
        [
            to_pos(cutting_radius, tool.cutting_length_mm),
            shank_bottom_right,
        ],
        stroke,
    );
    for step in 0..14 {
        let a = f64::from(step) / 14.0 * tool.total_length_mm;
        let b = (f64::from(step) + 0.5) / 14.0 * tool.total_length_mm;
        painter.line_segment(
            [to_pos(0.0, a), to_pos(0.0, b)],
            egui::Stroke::new(0.8, egui::Color32::LIGHT_GRAY),
        );
    }
    // Cotas legibles, calculadas en el sistema de la herramienta, no sobre el STL.
    let color = egui::Color32::LIGHT_GRAY;
    let font = egui::FontId::monospace(11.0);
    painter.text(
        rect.left_top() + egui::vec2(10.0, 10.0),
        egui::Align2::LEFT_TOP,
        format!("Corte Ø {:.4} mm", tool.geometry.diameter_mm()),
        font.clone(),
        color,
    );
    painter.text(
        rect.left_top() + egui::vec2(10.0, 26.0),
        egui::Align2::LEFT_TOP,
        format!("Mango Ø {:.4} mm", tool.shank_diameter_mm),
        font.clone(),
        color,
    );
    let dims_x = center_x + (largest_radius * scale) as f32 + 11.0;
    painter.line_segment(
        [egui::pos2(dims_x, bottom), egui::pos2(dims_x, end_cut.y)],
        egui::Stroke::new(1.0, color),
    );
    painter.text(
        egui::pos2(dims_x + 4.0, (bottom + end_cut.y) * 0.5),
        egui::Align2::LEFT_CENTER,
        format!("Lc {:.3}", tool.cutting_length_mm),
        font.clone(),
        color,
    );
    let dims_x2 = center_x - (largest_radius * scale) as f32 - 9.0;
    painter.line_segment(
        [egui::pos2(dims_x2, bottom), egui::pos2(dims_x2, end_free.y)],
        egui::Stroke::new(1.0, color),
    );
    painter.text(
        egui::pos2(dims_x2 - 4.0, (bottom + end_free.y) * 0.5),
        egui::Align2::RIGHT_CENTER,
        format!("L total {:.3}", tool.total_length_mm),
        font.clone(),
        color,
    );
    let tip = match &tool.geometry {
        ToolGeometry::Flat { .. } => "Punta plana".into(),
        ToolGeometry::BallNose {
            cutting_diameter_mm,
        } => format!("R esférico {:.4} mm", cutting_diameter_mm * 0.5),
        ToolGeometry::VBit {
            included_angle_deg, ..
        } => format!("V · ángulo TOTAL {:.3}°", included_angle_deg),
        ToolGeometry::ConicalFlat {
            tip_diameter_mm,
            included_angle_deg,
            ..
        } => format!(
            "Punta plana Ø {:.4} · ángulo TOTAL {:.3}°",
            tip_diameter_mm, included_angle_deg
        ),
        ToolGeometry::ConicalBall {
            tip_sphere_radius_mm,
            included_angle_deg,
            ..
        } => format!(
            "Punta esférica R {:.4} · ángulo TOTAL {:.3}°",
            tip_sphere_radius_mm, included_angle_deg
        ),
    };
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -7.0),
        egui::Align2::LEFT_BOTTOM,
        tip,
        font,
        color,
    );
    ui.small("Azul: tramo cortante · Gris: mango no cortante. Perfil axial, no simulación de colisiones.");
}

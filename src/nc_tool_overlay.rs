//! P2-D2.2: dibujo 3D ligero de la fresa seleccionada durante la reproduccion NC.
//! Coordenadas fisicas del modelo en milimetros; solo herramienta vertical 3 ejes.
//! El dibujo se superpone al material egui. No realiza test de profundidad,
//! no afecta al NC, al campo de alturas ni certifica ausencia de colisiones.

use crate::{
    navigation::Projection,
    project::Project,
    tools::model::{Tool, ToolGeometry},
};
use eframe::egui;

const ANGULAR_STEPS: usize = 24;
const BALL_STEPS: usize = 9;

#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub pan: glam::Vec3,
    pub projection: Projection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Cutting,
    Shank,
}

#[derive(Clone, Copy, Debug)]
struct Face {
    vertices: [[f64; 3]; 3],
    section: Section,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TipKind {
    Flat,
    Ball,
}

#[derive(Clone, Copy, Debug)]
struct Shape {
    tip: TipKind,
    cut_radius: f64,
    shank_radius: f64,
    cut_length: f64,
    total_length: f64,
}

impl Shape {
    fn from_tool(tool: &Tool) -> Option<Self> {
        tool.validate().ok()?;
        let tip = match &tool.geometry {
            ToolGeometry::Flat { .. } => TipKind::Flat,
            ToolGeometry::BallNose { .. } => TipKind::Ball,
            _ => return None,
        };
        Some(Self {
            tip,
            cut_radius: tool.geometry.diameter_mm() * 0.5,
            shank_radius: tool.shank_diameter_mm * 0.5,
            cut_length: tool.cutting_length_mm,
            total_length: tool.total_length_mm,
        })
    }
}

fn radial(radius: f64, z: f64, step: usize) -> [f64; 3] {
    let angle = std::f64::consts::TAU * step as f64 / ANGULAR_STEPS as f64;
    [radius * angle.cos(), radius * angle.sin(), z]
}

fn triangle(out: &mut Vec<Face>, vertices: [[f64; 3]; 3], section: Section) {
    let a = vertices[0];
    let b = vertices[1];
    let c = vertices[2];
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    if cross.iter().map(|x| x * x).sum::<f64>() > 1.0e-18 {
        out.push(Face { vertices, section });
    }
}

fn band(out: &mut Vec<Face>, lower: (f64, f64), upper: (f64, f64), section: Section) {
    for step in 0..ANGULAR_STEPS {
        let a = radial(lower.0, lower.1, step);
        let b = radial(lower.0, lower.1, step + 1);
        let c = radial(upper.0, upper.1, step + 1);
        let d = radial(upper.0, upper.1, step);
        triangle(out, [a, b, c], section);
        triangle(out, [a, c, d], section);
    }
}

fn cap(out: &mut Vec<Face>, radius: f64, z: f64, section: Section) {
    for step in 0..ANGULAR_STEPS {
        triangle(
            out,
            [
                [0.0, 0.0, z],
                radial(radius, z, step),
                radial(radius, z, step + 1),
            ],
            section,
        );
    }
}

/// Devuelve triangulos con Z=0 en la punta de la herramienta, Z positivo hacia el mango.
fn mesh(shape: Shape) -> Vec<Face> {
    let mut faces = Vec::with_capacity(ANGULAR_STEPS * (BALL_STEPS + 5) * 2);
    let r = shape.cut_radius;
    let last = match shape.tip {
        TipKind::Flat => {
            cap(&mut faces, r, 0.0, Section::Cutting);
            (r, 0.0)
        }
        TipKind::Ball => {
            let mut last = (0.0, 0.0);
            for step in 1..=BALL_STEPS {
                let angle = std::f64::consts::FRAC_PI_2 * step as f64 / BALL_STEPS as f64;
                let next = (r * angle.sin(), r * (1.0 - angle.cos()));
                band(&mut faces, last, next, Section::Cutting);
                last = next;
            }
            last
        }
    };
    band(&mut faces, last, (r, shape.cut_length), Section::Cutting);
    if shape.total_length > shape.cut_length {
        if (shape.shank_radius - r).abs() > 1.0e-10 {
            band(
                &mut faces,
                (r, shape.cut_length),
                (shape.shank_radius, shape.cut_length),
                Section::Shank,
            );
        }
        band(
            &mut faces,
            (shape.shank_radius, shape.cut_length),
            (shape.shank_radius, shape.total_length),
            Section::Shank,
        );
        cap(
            &mut faces,
            shape.shank_radius,
            shape.total_length,
            Section::Shank,
        );
    } else {
        cap(&mut faces, r, shape.cut_length, Section::Cutting);
    }
    faces
}

fn shade(vertices: &[[f64; 3]; 3], section: Section) -> egui::Color32 {
    let a = vertices[0];
    let b = vertices[1];
    let c = vertices[2];
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let magnitude = normal
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        .sqrt()
        .max(1.0e-20);
    let lit = ((normal[0] * 0.35 - normal[1] * 0.40 + normal[2] * 0.85) / magnitude)
        .abs()
        .clamp(0.0, 1.0);
    let intensity = 0.48 + 0.52 * lit;
    let rgb: [u8; 3] = match section {
        Section::Cutting => [110, 218, 233],
        Section::Shank => [215, 220, 228],
    };
    egui::Color32::from_rgb(
        (f64::from(rgb[0]) * intensity) as u8,
        (f64::from(rgb[1]) * intensity) as u8,
        (f64::from(rgb[2]) * intensity) as u8,
    )
}

/// Superpone un modelo de fresa al material, sin escribir sobre la trayectoria ni el STL.
pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    project: &Project,
    tool: &Tool,
    tip_work_mm: [f64; 3],
    camera: Camera,
) {
    let Some(shape) = Shape::from_tool(tool) else {
        return;
    };
    let Some(model) = &project.model else { return };
    let Some(stock) = &project.stock else { return };
    let Ok(tip) = project
        .origin
        .work_to_model(stock, &model.bounds, tip_work_mm)
    else {
        return;
    };
    let mvp = crate::navigation::view_projection_mode(
        camera.yaw,
        camera.pitch,
        camera.distance,
        rect.width() / rect.height().max(1.0),
        camera.pan,
        camera.projection,
    );
    let mut projected = Vec::new();
    for face in mesh(shape) {
        let mut polygon = [egui::Pos2::ZERO; 3];
        let mut depth = 0.0_f32;
        let mut valid = true;
        for (index, local) in face.vertices.iter().enumerate() {
            let xyz = std::array::from_fn(|axis| tip[axis] + local[axis]);
            let Some(normalized) = model.visual_point(xyz) else {
                valid = false;
                break;
            };
            let clip = mvp * normalized.extend(1.0);
            if !clip.is_finite() || clip.w <= 0.0 {
                valid = false;
                break;
            }
            let ndc = clip.truncate() / clip.w;
            if !ndc.is_finite() || !(-1.0..=1.0).contains(&ndc.z) {
                valid = false;
                break;
            }
            depth += ndc.z;
            polygon[index] = rect.center()
                + egui::vec2(ndc.x * rect.width() * 0.5, -ndc.y * rect.height() * 0.5);
        }
        if valid {
            projected.push((depth / 3.0, polygon, shade(&face.vertices, face.section)));
        }
    }
    // En egui no hay buffer Z. Ordenar las caras de atras hacia delante.
    projected.sort_by(|a, b| b.0.total_cmp(&a.0));
    let painter = ui.painter().with_clip_rect(rect);
    for (_, polygon, color) in projected {
        painter.add(egui::Shape::convex_polygon(
            polygon.to_vec(),
            color,
            egui::Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(faces: &[Face]) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for face in faces {
            for p in face.vertices {
                for axis in 0..3 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
            }
        }
        (min, max)
    }

    #[test]
    fn flat_cutter_has_real_radius_and_tip_at_zero() {
        let faces = mesh(Shape {
            tip: TipKind::Flat,
            cut_radius: 3.0,
            shank_radius: 3.0,
            cut_length: 15.0,
            total_length: 35.0,
        });
        let (min, max) = bounds(&faces);
        assert!(!faces.is_empty());
        assert!((min[2] - 0.0).abs() < 1.0e-9);
        assert!((max[2] - 35.0).abs() < 1.0e-9);
        assert!((min[0] + 3.0).abs() < 1.0e-9);
        assert!((max[0] - 3.0).abs() < 1.0e-9);
        assert!(faces.iter().any(|f| f.section == Section::Shank));
    }

    #[test]
    fn ball_cutter_has_round_tip_and_shank() {
        let faces = mesh(Shape {
            tip: TipKind::Ball,
            cut_radius: 2.0,
            shank_radius: 2.5,
            cut_length: 10.0,
            total_length: 30.0,
        });
        let (min, max) = bounds(&faces);
        assert!((min[2] - 0.0).abs() < 1.0e-9);
        assert!((max[2] - 30.0).abs() < 1.0e-9);
        assert!((max[0] - 2.5).abs() < 1.0e-9);
        assert!(faces.iter().any(|face| {
            face.vertices
                .iter()
                .any(|point| point[2] > 0.0 && point[2] < 2.0)
        }));
    }

    #[test]
    fn zero_shank_extension_needs_no_shank_faces() {
        let faces = mesh(Shape {
            tip: TipKind::Flat,
            cut_radius: 1.5,
            shank_radius: 1.5,
            cut_length: 8.0,
            total_length: 8.0,
        });
        assert!(faces.iter().all(|face| face.section == Section::Cutting));
        assert_eq!(bounds(&faces).1[2], 8.0);
    }
}

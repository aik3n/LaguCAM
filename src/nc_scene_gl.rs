//! P2-D3: una escena GL opaca para material 2.5D, fresa y trayectoria NC.
//! La geometria y el render son transitorios: no alteran CAM, NC ni heightfield.
//! Las superficies comparten el mismo depth buffer; sin ordenacion de triangulos 2D.

use crate::{
    project::Project,
    tools::model::{Tool, ToolGeometry},
};
use cam_libre_visor::material_sim::MaterialHeightfield;
use eframe::glow;
use glow::HasContext as _;

// 192x192 -> ~220k vertices como maximo para la cara superior.
const MAX_RENDER_NODES: usize = 192;
const CIRCLE_STEPS: usize = 32;
const BALL_STEPS: usize = 12;
const STRIDE: usize = 6; // posicion normalizada XYZ + color RGB

/// No contiene punteros GPU: se prepara fuera del callback OpenGL.
#[derive(Default)]
pub struct Scene {
    solid: Vec<f32>,
    tool: Vec<f32>,
    lines: Vec<f32>,
}

fn shade(a: [f64; 3], b: [f64; 3], c: [f64; 3], base: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
    let light = if length > 1.0e-18 {
        ((n[0] * 0.25 + n[1] * (-0.4) + n[2] * 0.88).abs() / length) as f32
    } else {
        0.0
    };
    let factor = 0.45 + 0.55 * light;
    base.map(|x| x * factor)
}

impl Scene {
    fn triangle(&mut self, project: &Project, xyz: [[f64; 3]; 3], color: [f32; 3], tool: bool) {
        let Some(model) = &project.model else {
            return;
        };
        let mut points = [[0.0f32; 3]; 3];
        for (index, point) in xyz.iter().enumerate() {
            let Some(normalized) = model.visual_point(*point) else {
                return;
            };
            points[index] = normalized.to_array();
        }
        let rgb = shade(xyz[0], xyz[1], xyz[2], color);
        let target = if tool {
            &mut self.tool
        } else {
            &mut self.solid
        };
        for point in points {
            target.extend_from_slice(&point);
            target.extend_from_slice(&rgb);
        }
    }

    fn quad(
        &mut self,
        project: &Project,
        a: [f64; 3],
        b: [f64; 3],
        c: [f64; 3],
        d: [f64; 3],
        color: [f32; 3],
        tool: bool,
    ) {
        self.triangle(project, [a, b, c], color, tool);
        self.triangle(project, [a, c, d], color, tool);
    }

    pub fn from_material(project: &Project, field: &MaterialHeightfield) -> Option<Self> {
        project.model.as_ref()?;
        let [nx, ny] = field.cells;
        if nx < 2 || ny < 2 {
            return None;
        }
        let rx = nx.min(MAX_RENDER_NODES);
        let ry = ny.min(MAX_RENDER_NODES);
        let xidx = |i: usize| -> usize { i * (nx - 1) / (rx - 1) };
        let yidx = |j: usize| -> usize { j * (ny - 1) / (ry - 1) };
        let xy = |ix: usize, iy: usize| -> [f64; 2] {
            [
                field.stock.min_mm[0] + field.stock.size_mm[0] * ix as f64 / (nx - 1) as f64,
                field.stock.min_mm[1] + field.stock.size_mm[1] * iy as f64 / (ny - 1) as f64,
            ]
        };
        let point = |ix: usize, iy: usize| -> [f64; 3] {
            let p = xy(ix, iy);
            [p[0], p[1], field.height_at(ix, iy)]
        };
        let mut scene = Self::default();
        // No usar indices fuera de la malla fisica: la GPU recorta los triangulos.
        for j in 0..ry - 1 {
            for i in 0..rx - 1 {
                let (ix0, ix1, iy0, iy1) = (xidx(i), xidx(i + 1), yidx(j), yidx(j + 1));
                let a = point(ix0, iy0);
                let b = point(ix1, iy0);
                let c = point(ix1, iy1);
                let d = point(ix0, iy1);
                let cut = field.is_cut(ix0, iy0)
                    || field.is_cut(ix1, iy0)
                    || field.is_cut(ix1, iy1)
                    || field.is_cut(ix0, iy1);
                let color = if cut {
                    [0.20, 0.65, 0.83]
                } else {
                    [0.72, 0.51, 0.30]
                };
                scene.quad(project, a, b, c, d, color, false);
            }
        }
        // Faldones en el limite real del bruto. No formar falsos triangulos
        // proyectados a egui ni usar Pos2::ZERO como vertice de respaldo.
        let bottom = field.stock.min_mm[2];
        for j in 0..ry - 1 {
            let (iy0, iy1) = (yidx(j), yidx(j + 1));
            for ix in [0, nx - 1] {
                let a = point(ix, iy0);
                let b = point(ix, iy1);
                scene.quad(
                    project,
                    a,
                    b,
                    [b[0], b[1], bottom],
                    [a[0], a[1], bottom],
                    [0.32, 0.39, 0.43],
                    false,
                );
            }
        }
        for i in 0..rx - 1 {
            let (ix0, ix1) = (xidx(i), xidx(i + 1));
            for iy in [0, ny - 1] {
                let a = point(ix0, iy);
                let b = point(ix1, iy);
                scene.quad(
                    project,
                    a,
                    b,
                    [b[0], b[1], bottom],
                    [a[0], a[1], bottom],
                    [0.32, 0.39, 0.43],
                    false,
                );
            }
        }
        Some(scene)
    }

    fn ring_point(center: [f64; 3], radius: f64, height: f64, step: usize) -> [f64; 3] {
        let angle = std::f64::consts::TAU * step as f64 / CIRCLE_STEPS as f64;
        [
            center[0] + radius * angle.cos(),
            center[1] + radius * angle.sin(),
            center[2] + height,
        ]
    }

    fn band(
        &mut self,
        project: &Project,
        center: [f64; 3],
        lower: (f64, f64),
        upper: (f64, f64),
        color: [f32; 3],
    ) {
        for step in 0..CIRCLE_STEPS {
            let a = Self::ring_point(center, lower.0, lower.1, step);
            let b = Self::ring_point(center, lower.0, lower.1, step + 1);
            let c = Self::ring_point(center, upper.0, upper.1, step + 1);
            let d = Self::ring_point(center, upper.0, upper.1, step);
            self.quad(project, a, b, c, d, color, true);
        }
    }

    pub fn append_tool(&mut self, project: &Project, tool: &Tool, tip_work_mm: [f64; 3]) {
        if tool.validate().is_err() {
            return;
        }
        let (Some(model), Some(stock)) = (&project.model, &project.stock) else {
            return;
        };
        let Ok(center) = project
            .origin
            .work_to_model(stock, &model.bounds, tip_work_mm)
        else {
            return;
        };
        let r = tool.geometry.diameter_mm() / 2.0;
        let sr = tool.shank_diameter_mm / 2.0;
        let cut = tool.cutting_length_mm;
        let total = tool.total_length_mm;
        match &tool.geometry {
            ToolGeometry::Flat { .. } => {
                self.band(project, center, (r, 0.0), (r, cut), [0.96, 0.72, 0.13]);
                // Base plana, triangulos con centro en Z punta.
                for i in 0..CIRCLE_STEPS {
                    self.triangle(
                        project,
                        [
                            center,
                            Self::ring_point(center, r, 0.0, i + 1),
                            Self::ring_point(center, r, 0.0, i),
                        ],
                        [0.90, 0.66, 0.10],
                        true,
                    );
                }
            }
            ToolGeometry::BallNose { .. } => {
                let mut lower = (0.0, 0.0);
                for i in 1..=BALL_STEPS {
                    let angle = std::f64::consts::FRAC_PI_2 * i as f64 / BALL_STEPS as f64;
                    let upper = (r * angle.sin(), r * (1.0 - angle.cos()));
                    self.band(project, center, lower, upper, [0.96, 0.72, 0.13]);
                    lower = upper;
                }
                self.band(project, center, (r, r), (r, cut), [0.96, 0.72, 0.13]);
            }
            _ => return,
        }
        if total > cut {
            self.band(project, center, (sr, cut), (sr, total), [0.67, 0.71, 0.76]);
            // Tapa del mango.
            let top = [center[0], center[1], center[2] + total];
            for i in 0..CIRCLE_STEPS {
                self.triangle(
                    project,
                    [
                        top,
                        Self::ring_point(center, sr, total, i),
                        Self::ring_point(center, sr, total, i + 1),
                    ],
                    [0.67, 0.71, 0.76],
                    true,
                );
            }
        }
    }

    pub fn add_line(&mut self, project: &Project, from: [f64; 3], to: [f64; 3], color: [f32; 3]) {
        let Some(model) = &project.model else {
            return;
        };
        let (Some(a), Some(b)) = (model.visual_point(from), model.visual_point(to)) else {
            return;
        };
        for v in [a, b] {
            self.lines.extend_from_slice(&v.to_array());
            self.lines.extend_from_slice(&color);
        }
    }

    pub fn triangles(&self) -> usize {
        (self.solid.len() + self.tool.len()) / (STRIDE * 3)
    }
}

pub struct Renderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    u_mvp: glow::UniformLocation,
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vs = gl
                .create_shader(glow::VERTEX_SHADER)
                .inspect_err(|_| gl.delete_program(program))?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER).inspect_err(|_| {
                gl.delete_shader(vs);
                gl.delete_program(program);
            })?;
            gl.shader_source(
                vs,
                r#"#version 330 core
layout(location=0) in vec3 position;
layout(location=1) in vec3 color;
uniform mat4 u_mvp;
out vec3 v_color;
void main(){ gl_Position=u_mvp*vec4(position,1.0); v_color=color; }
"#,
            );
            gl.shader_source(
                fs,
                r#"#version 330 core
in vec3 v_color;
out vec4 fragColor;
void main(){ fragColor=vec4(v_color,1.0); }
"#,
            );
            gl.compile_shader(vs);
            gl.compile_shader(fs);
            let mut error = String::new();
            if !gl.get_shader_compile_status(vs) {
                error = gl.get_shader_info_log(vs);
            }
            if !gl.get_shader_compile_status(fs) {
                error = gl.get_shader_info_log(fs);
            }
            if error.is_empty() {
                gl.attach_shader(program, vs);
                gl.attach_shader(program, fs);
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    error = gl.get_program_info_log(program);
                }
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !error.is_empty() {
                gl.delete_program(program);
                return Err(format!("Simulacion GL: {error}"));
            }
            let Some(u_mvp) = gl.get_uniform_location(program, "u_mvp") else {
                gl.delete_program(program);
                return Err("Simulacion GL: uniform u_mvp ausente".into());
            };
            let vao = gl
                .create_vertex_array()
                .inspect_err(|_| gl.delete_program(program))?;
            let vbo = gl.create_buffer().inspect_err(|_| {
                gl.delete_vertex_array(vao);
                gl.delete_program(program);
            })?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            Ok(Self {
                program,
                vao,
                vbo,
                u_mvp,
            })
        }
    }

    unsafe fn draw(&self, gl: &glow::Context, vertices: &[f32], mode: u32) {
        if vertices.is_empty() {
            return;
        }
        unsafe {
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices),
                glow::DYNAMIC_DRAW,
            );
            gl.draw_arrays(mode, 0, (vertices.len() / STRIDE) as i32);
        }
    }

    /// En el callback egui_glow, con FBO y viewport ya establecidos por eframe.
    /// Se limpia solo el viewport con scissor para evitar mezcla con STL previo.
    pub fn paint(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        camera: crate::stock_render::Camera,
    ) {
        let mvp = crate::navigation::view_projection_mode(
            camera.yaw,
            camera.pitch,
            camera.zoom,
            camera.aspect,
            camera.pan,
            camera.projection,
        );
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.draw(gl, &scene.solid, glow::TRIANGLES);
            gl.depth_mask(false);
            self.draw(gl, &scene.lines, glow::LINES);
            gl.depth_mask(true);
            self.draw(gl, &scene.tool, glow::TRIANGLES);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
        }
    }

    pub fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_buffer(self.vbo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nodes_and_triangle_budget_are_bounded() {
        assert_eq!(MAX_RENDER_NODES, 192);
        assert!(MAX_RENDER_NODES * MAX_RENDER_NODES * 6 * STRIDE < 1_400_000);
    }
    #[test]
    fn flat_normal_has_finite_shading() {
        let c = shade([0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 0.5, 0.]);
        assert!(c.iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0));
    }
}

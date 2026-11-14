//! Renderizado ligero del bruto: se dibuja después de la malla opaca, con
//! comparación de profundidad y sin escribir sobre el depth buffer del STL.
//! No modifica los VBO de la malla ni las coordenadas reales de proyecto.
use crate::project::{Project, display::StockDisplay};
use eframe::glow;
use glam::{Mat4, Vec3};
use glow::HasContext as _;

/// Solo geometría visual en coordenadas normalizadas. La malla STL se conserva aparte.
#[derive(Clone)]
pub struct StockGeometry {
    faces: [f32; 108], // 12 triángulos, 3 vértices, 3 coordenadas
    edges: [f32; 72],  // 12 aristas, 2 vértices, 3 coordenadas
    display: StockDisplay,
}

impl StockGeometry {
    pub fn from_project(project: &Project) -> Option<Self> {
        let model = project.model.as_ref()?;
        let stock = project.stock.as_ref()?;
        if !stock.visible || stock.validate().is_err() {
            return None;
        }
        let min = stock.min;
        let max = stock.max()?;
        let original = [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [min[0], max[1], max[2]],
            [max[0], max[1], max[2]],
        ];
        let mut points = [[0.0_f32; 3]; 8];
        for (index, point) in original.into_iter().enumerate() {
            points[index] = model.visual_point(point)?.to_array();
        }
        // Caras CCW vistas desde fuera: se dibujarán solo las orientadas a cámara.
        const FACES: [[usize; 3]; 12] = [
            [0, 2, 1],
            [1, 2, 3], // Z-
            [4, 5, 6],
            [5, 7, 6], // Z+
            [0, 4, 2],
            [2, 4, 6], // X-
            [1, 3, 5],
            [3, 7, 5], // X+
            [0, 1, 4],
            [1, 5, 4], // Y-
            [2, 6, 3],
            [3, 6, 7], // Y+
        ];
        const EDGES: [[usize; 2]; 12] = [
            [0, 1],
            [0, 2],
            [1, 3],
            [2, 3],
            [4, 5],
            [4, 6],
            [5, 7],
            [6, 7],
            [0, 4],
            [1, 5],
            [2, 6],
            [3, 7],
        ];
        let mut faces = [0.0_f32; 108];
        for (triangle_index, indices) in FACES.into_iter().enumerate() {
            for (vertex_index, index) in indices.into_iter().enumerate() {
                let start = (triangle_index * 3 + vertex_index) * 3;
                faces[start..start + 3].copy_from_slice(&points[index]);
            }
        }
        let mut edges = [0.0_f32; 72];
        for (edge_index, indices) in EDGES.into_iter().enumerate() {
            for (vertex_index, index) in indices.into_iter().enumerate() {
                let start = (edge_index * 2 + vertex_index) * 3;
                edges[start..start + 3].copy_from_slice(&points[index]);
            }
        }
        Some(Self {
            faces,
            edges,
            display: project.stock_display,
        })
    }
}

/// Parámetros de la cámara, compartidos con la vista STL.
/// Solo se usan para dibujar; no alteran coordenadas CAM.
#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub aspect: f32,
    pub pan: Vec3,
}

pub struct StockRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    u_mvp: glow::UniformLocation,
    u_color: glow::UniformLocation,
}

impl StockRenderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vertex_shader = match gl.create_shader(glow::VERTEX_SHADER) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.shader_source(
                vertex_shader,
                r#"#version 330 core
layout(location = 0) in vec3 a_pos;
uniform mat4 u_mvp;
void main() { gl_Position = u_mvp * vec4(a_pos, 1.0); }
"#,
            );
            gl.compile_shader(vertex_shader);
            if !gl.get_shader_compile_status(vertex_shader) {
                let error = gl.get_shader_info_log(vertex_shader);
                gl.delete_shader(vertex_shader);
                gl.delete_program(program);
                return Err(format!("Bruto, vertex shader: {error}"));
            }
            let fragment_shader = match gl.create_shader(glow::FRAGMENT_SHADER) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_shader(vertex_shader);
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.shader_source(
                fragment_shader,
                r#"#version 330 core
uniform vec4 u_color;
out vec4 out_color;
void main() { out_color = u_color; }
"#,
            );
            gl.compile_shader(fragment_shader);
            if !gl.get_shader_compile_status(fragment_shader) {
                let error = gl.get_shader_info_log(fragment_shader);
                gl.delete_shader(fragment_shader);
                gl.delete_shader(vertex_shader);
                gl.delete_program(program);
                return Err(format!("Bruto, fragment shader: {error}"));
            }
            gl.attach_shader(program, vertex_shader);
            gl.attach_shader(program, fragment_shader);
            gl.link_program(program);
            let linked = gl.get_program_link_status(program);
            let link_error = gl.get_program_info_log(program);
            gl.detach_shader(program, vertex_shader);
            gl.detach_shader(program, fragment_shader);
            gl.delete_shader(vertex_shader);
            gl.delete_shader(fragment_shader);
            if !linked {
                gl.delete_program(program);
                return Err(format!("Bruto, shader link: {link_error}"));
            }
            let (Some(u_mvp), Some(u_color)) = (
                gl.get_uniform_location(program, "u_mvp"),
                gl.get_uniform_location(program, "u_color"),
            ) else {
                gl.delete_program(program);
                return Err("Bruto: no se encontraron los uniforms del shader".into());
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
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            Ok(Self {
                program,
                vao,
                vbo,
                u_mvp,
                u_color,
            })
        }
    }

    /// El render de malla ya ha escrito su profundidad; no la limpiamos ni la reemplazamos.
    pub fn paint(&mut self, gl: &glow::Context, geometry: Option<&StockGeometry>, camera: Camera) {
        let Some(geometry) = geometry else {
            return;
        };
        let Camera {
            yaw,
            pitch,
            zoom,
            aspect,
            pan,
        } = camera;
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
        let rgb = geometry.display.color.rgb();
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(false);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            if geometry.display.mode.has_faces() && geometry.display.opacity_percent > 0 {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(glow::BACK);
                gl.front_face(glow::CCW);
                self.draw(
                    gl,
                    &geometry.faces,
                    glow::TRIANGLES,
                    rgb,
                    geometry.display.opacity(),
                );
                gl.disable(glow::CULL_FACE);
            }
            if geometry.display.mode.has_edges() {
                // Las aristas también se someten a la profundidad del STL.
                self.draw(gl, &geometry.edges, glow::LINES, rgb, 1.0);
            }
            // Restaurar la mezcla premultiplicada usada por egui/glow.
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.depth_mask(true);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
        }
    }

    fn draw(
        &self,
        gl: &glow::Context,
        positions: &[f32],
        primitive: u32,
        rgb: [f32; 3],
        alpha: f32,
    ) {
        unsafe {
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(positions),
                glow::DYNAMIC_DRAW,
            );
            gl.uniform_4_f32(Some(&self.u_color), rgb[0], rgb[1], rgb[2], alpha);
            gl.draw_arrays(primitive, 0, (positions.len() / 3) as i32);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
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
    use crate::{
        project::{model::ModelReference, stock::Stock},
        stl::Bounds,
    };

    fn reference_project() -> Project {
        let mut project = Project::default();
        let bounds = Bounds {
            min: [10.0, 20.0, 30.0],
            max: [20.0, 30.0, 40.0],
        };
        project.model = Some(ModelReference {
            name: "reference".into(),
            bounds,
            triangles: 12,
            size_bytes: 684,
        });
        project.stock = Some(Stock::from_bounds(&bounds));
        project
    }

    #[test]
    fn cube_faces_point_outwards_and_real_bounds_stay_unchanged() {
        let project = reference_project();
        let geometry = StockGeometry::from_project(&project).unwrap();
        assert_eq!(geometry.faces.len(), 108);
        assert_eq!(geometry.edges.len(), 72);
        for triangle in geometry.faces.as_chunks::<9>().0 {
            let a = Vec3::new(triangle[0], triangle[1], triangle[2]);
            let b = Vec3::new(triangle[3], triangle[4], triangle[5]);
            let c = Vec3::new(triangle[6], triangle[7], triangle[8]);
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0);
        }
        assert_eq!(project.stock.as_ref().unwrap().min, [10.0, 20.0, 30.0]);
        assert_eq!(
            project.model.as_ref().unwrap().bounds.max,
            [20.0, 30.0, 40.0]
        );
    }

    #[test]
    fn invisible_stock_has_no_overlay() {
        let mut project = reference_project();
        project.stock.as_mut().unwrap().visible = false;
        assert!(StockGeometry::from_project(&project).is_none());
    }
}

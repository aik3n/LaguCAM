//! Renderizador 3D independiente del lector STL y del resto de la interfaz.
use crate::preview::PreviewMesh;
use eframe::glow;
use glam::{Mat4, Vec3};
use glow::HasContext as _;

pub struct Render3d {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    elements: i32,
    pending: Option<PreviewMesh>,
    error: Option<String>,
}

impl Render3d {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vs = gl
                .create_shader(glow::VERTEX_SHADER)
                .inspect_err(|_| gl.delete_program(program))?;
            gl.shader_source(
                vs,
                r#"#version 330 core
layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec3 a_normal;
uniform mat4 u_mvp;
out vec3 v_normal;
out float v_height;
void main() {
  gl_Position = u_mvp * vec4(a_pos, 1.0);
  v_normal = a_normal;
  v_height = a_pos.z;
}
"#,
            );
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                let error = gl.get_shader_info_log(vs);
                gl.delete_shader(vs);
                gl.delete_program(program);
                return Err(format!("Vertex shader: {error}"));
            }
            let fs = gl.create_shader(glow::FRAGMENT_SHADER).inspect_err(|_| {
                gl.delete_shader(vs);
                gl.delete_program(program);
            })?;
            gl.shader_source(
                fs,
                r#"#version 330 core
in vec3 v_normal;
in float v_height;
out vec4 out_color;
void main() {
  vec3 n = normalize(v_normal);
  vec3 light = normalize(vec3(0.35, -0.5, 0.8));
  float diffuse = max(dot(n, light), 0.0);
  vec3 low = vec3(0.16, 0.34, 0.27);
  vec3 high = vec3(0.80, 0.73, 0.54);
  float t = clamp(0.5 + v_height * 0.7, 0.0, 1.0);
  vec3 base = mix(low, high, t);
  out_color = vec4(base * (0.35 + 0.65 * diffuse), 1.0);
}
"#,
            );
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                let error = gl.get_shader_info_log(fs);
                gl.delete_shader(fs);
                gl.delete_shader(vs);
                gl.delete_program(program);
                return Err(format!("Fragment shader: {error}"));
            }
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            let linked = gl.get_program_link_status(program);
            let link_error = gl.get_program_info_log(program);
            gl.detach_shader(program, vs);
            gl.detach_shader(program, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !linked {
                gl.delete_program(program);
                return Err(format!("Shader link: {link_error}"));
            }

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
            Ok(Self {
                program,
                vao,
                vbo,
                elements: 0,
                pending: None,
                error: None,
            })
        }
    }

    /// Se invoca desde la UI, pero la subida a GPU se hace dentro del callback OpenGL.
    pub fn replace_mesh(&mut self, mesh: PreviewMesh) {
        self.pending = Some(mesh);
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub fn paint(
        &mut self,
        gl: &glow::Context,
        yaw: f32,
        pitch: f32,
        distance: f32,
        aspect: f32,
        pan: Vec3,
    ) {
        unsafe {
            if let Some(mesh) = self.pending.take() {
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(&mesh.vertices),
                    glow::STATIC_DRAW,
                );
                self.elements = (mesh.vertices.len() / 6) as i32;
                let error = gl.get_error();
                if error != glow::NO_ERROR {
                    self.elements = 0;
                    self.error = Some(format!("Subida de malla fallida (OpenGL 0x{error:04x})"));
                }
                gl.bind_vertex_array(None);
            }
            gl.enable(glow::SCISSOR_TEST); // la región recortada la establece egui
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            gl.clear_color(0.075, 0.09, 0.12, 1.0);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            if self.elements > 0 {
                let distance = fit_distance(aspect) * distance;
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
                gl.use_program(Some(self.program));
                gl.uniform_matrix_4_f32_slice(
                    gl.get_uniform_location(self.program, "u_mvp").as_ref(),
                    false,
                    &mvp.to_cols_array(),
                );
                gl.bind_vertex_array(Some(self.vao));
                gl.draw_arrays(glow::TRIANGLES, 0, self.elements);
                gl.bind_vertex_array(None);
            }
            gl.disable(glow::DEPTH_TEST);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
        }
    }
}

/// Encaja la esfera que contiene el cubo normalizado [-1,1]^3 también en ventanas estrechas.
pub fn fit_distance(aspect: f32) -> f32 {
    let vertical = 22.5_f32.to_radians();
    let horizontal = (vertical.tan() * aspect.max(0.001)).atan();
    3.0_f32.sqrt() / vertical.min(horizontal).sin() * 1.05
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_contains_bounding_sphere_in_portrait_and_landscape() {
        for aspect in [0.1_f32, 0.5, 1.0, 2.0, 5.0] {
            let angle = 22.5_f32
                .to_radians()
                .min((22.5_f32.to_radians().tan() * aspect).atan());
            assert!(fit_distance(aspect) * angle.sin() > 3.0_f32.sqrt());
        }
    }
}

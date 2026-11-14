//! Renderizador OpenGL con VBO independientes. La malla anterior sigue visible
//! hasta confirmar la subida completa del reemplazo.
use crate::preview::PreviewMesh;
use eframe::glow;
use glam::Vec3;
use glow::HasContext as _;

struct GpuChunk {
    buffer: glow::Buffer,
    elements: i32,
}

pub struct Render3d {
    program: glow::Program,
    model_color: [f32; 3],
    vao: glow::VertexArray,
    active: Vec<GpuChunk>,
    staging: Vec<GpuChunk>,
    pending: Option<PreviewMesh>,
    finishing: bool,
    aborting: bool,
    clear_requested: bool,
    completed: bool,
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
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
uniform mat4 u_mvp;
out vec3 v_normal;
out float v_height;
void main() {
    gl_Position = u_mvp * vec4(a_pos, 1.0);
    v_normal = a_normal;
    v_height = a_pos.z;
}"#,
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
uniform vec3 u_model_color;
out vec4 out_color;
void main() {
    vec3 n = normalize(v_normal);
    vec3 light = normalize(vec3(0.35, -0.5, 0.8));
    float diffuse = max(dot(n, light), 0.0);
    float t = clamp(0.5 + v_height * 0.7, 0.0, 1.0);
    vec3 base = u_model_color * (0.75 + 0.25 * t);
    out_color = vec4(base * (0.35 + 0.65 * diffuse), 1.0);
}"#,
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
            Ok(Self {
                model_color: [160.0 / 255.0, 180.0 / 255.0, 150.0 / 255.0],
                program,
                vao,
                active: vec![],
                staging: vec![],
                pending: None,
                finishing: false,
                aborting: false,
                clear_requested: false,
                completed: false,
                error: None,
            })
        }
    }

    /// Solo cambia un uniform; no se reenvían vertices del STL.
    pub fn set_model_color(&mut self, color: [u8; 3]) {
        self.model_color = color.map(|v| f32::from(v) / 255.0);
    }

    pub fn begin_upload(&mut self) {
        self.aborting = true;
        self.pending = None;
        self.finishing = false;
        self.completed = false;
        self.error = None;
    }
    pub fn can_accept_chunk(&self) -> bool {
        self.pending.is_none()
    }
    pub fn queue_chunk(&mut self, chunk: PreviewMesh) {
        self.pending = Some(chunk);
    }
    pub fn finish_upload(&mut self) {
        self.finishing = true;
    }
    /// Nuevo proyecto: se retiran los VBO solamente dentro del callback GL.
    pub fn clear_scene(&mut self) {
        self.abort_upload();
        self.clear_requested = true;
    }

    pub fn abort_upload(&mut self) {
        self.aborting = true;
        self.pending = None;
        self.finishing = false;
        self.completed = false;
    }
    pub fn take_completed(&mut self) -> bool {
        std::mem::take(&mut self.completed)
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
            if self.clear_requested {
                for chunk in self.active.drain(..) {
                    gl.delete_buffer(chunk.buffer);
                }
                self.clear_requested = false;
            }
            if self.aborting {
                for chunk in self.staging.drain(..) {
                    gl.delete_buffer(chunk.buffer);
                }
                self.aborting = false;
            }
            if let Some(chunk) = self.pending.take() {
                let upload = (|| -> Result<(), String> {
                    let buffer = gl.create_buffer()?;
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        bytemuck::cast_slice(&chunk.vertices),
                        glow::STATIC_DRAW,
                    );
                    let error = gl.get_error();
                    if error != glow::NO_ERROR {
                        gl.delete_buffer(buffer);
                        return Err(format!(
                            "Error al transferir bloque a GPU (OpenGL 0x{error:04x})"
                        ));
                    }
                    self.staging.push(GpuChunk {
                        buffer,
                        elements: (chunk.vertices.len() / 6) as i32,
                    });
                    Ok(())
                })();
                if let Err(error) = upload {
                    self.error = Some(error);
                    self.abort_upload();
                    for chunk in self.staging.drain(..) {
                        gl.delete_buffer(chunk.buffer);
                    }
                    self.aborting = false;
                }
            }
            if self.finishing && self.pending.is_none() {
                for chunk in self.active.drain(..) {
                    gl.delete_buffer(chunk.buffer);
                }
                self.active = std::mem::take(&mut self.staging);
                self.finishing = false;
                self.completed = true;
            }

            gl.enable(glow::SCISSOR_TEST);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            // El cielo lo dibuja egui. Se limpia solo la profundidad.
            // No modificar el streaming ni los bloques GPU del STL.
            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            if !self.active.is_empty() {
                let mvp = crate::navigation::view_projection(yaw, pitch, distance, aspect, pan);
                gl.use_program(Some(self.program));
                gl.uniform_3_f32(
                    gl.get_uniform_location(self.program, "u_model_color")
                        .as_ref(),
                    self.model_color[0],
                    self.model_color[1],
                    self.model_color[2],
                );
                gl.uniform_matrix_4_f32_slice(
                    gl.get_uniform_location(self.program, "u_mvp").as_ref(),
                    false,
                    &mvp.to_cols_array(),
                );
                gl.bind_vertex_array(Some(self.vao));
                gl.enable_vertex_attrib_array(0);
                gl.enable_vertex_attrib_array(1);
                for chunk in &self.active {
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(chunk.buffer));
                    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
                    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
                    gl.draw_arrays(glow::TRIANGLES, 0, chunk.elements);
                }
                gl.bind_vertex_array(None);
            }
            gl.disable(glow::DEPTH_TEST);
        }
    }

    pub fn destroy(&mut self, gl: &glow::Context) {
        unsafe {
            for chunk in self.active.drain(..).chain(self.staging.drain(..)) {
                gl.delete_buffer(chunk.buffer);
            }
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
        }
    }
}

pub fn fit_distance(aspect: f32) -> f32 {
    crate::navigation::fit_distance(aspect)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_contains_sphere_in_narrow_and_wide_windows() {
        for aspect in [0.1_f32, 0.5, 1.0, 2.0, 5.0] {
            let angle = 22.5_f32
                .to_radians()
                .min((22.5_f32.to_radians().tan() * aspect).atan());
            assert!(fit_distance(aspect) * angle.sin() > 3.0_f32.sqrt());
        }
    }
}

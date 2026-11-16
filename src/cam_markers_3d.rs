//! Mallas 3D de marcadores visuales CAM (inicio: cono, final: esfera).
//! Dibujadas en OpenGL y sometidas al depth buffer del STL. No son geometria CAM.
use crate::stock_render::Camera;
use eframe::glow;
use glam::Vec3;
use glow::HasContext as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    StartCone,
    FinishSphere,
}

#[derive(Clone, Copy, Debug)]
pub struct Marker3d {
    /// Centro en coordenadas normalizadas del visor (NO coordenadas CAM).
    pub position: Vec3,
    pub kind: MarkerKind,
    /// Eje del cono en el espacio normalizado del visor.
    pub direction: Vec3,
}

/// Escala visual en el espacio normalizado del modelo; acompana al zoom.
/// El cono tiene radio 0.055 y altura 0.11; la esfera, radio 0.055.
const MARKER_SCALE: f32 = 0.055;

pub struct MarkerRenderer3d {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    u_mvp: glow::UniformLocation,
    u_center: glow::UniformLocation,
    u_scale: glow::UniformLocation,
    u_axis: glow::UniformLocation,
    cone_vertices: i32,
    sphere_offset: i32,
    sphere_vertices: i32,
}

impl MarkerRenderer3d {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vertex = match gl.create_shader(glow::VERTEX_SHADER) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.shader_source(
                vertex,
                r#"#version 330 core
layout(location=0) in vec3 a_position;
layout(location=1) in vec3 a_normal;
uniform mat4 u_mvp;
uniform vec3 u_center;
uniform float u_scale;
uniform vec3 u_axis;
out vec3 v_normal;
void main() {
    // Base local XY, punta local +Z. Rotar +Z hasta la direccion inicial.
    vec3 axis = normalize(u_axis);
    vec3 reference = abs(axis.z) < 0.9 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 tangent = normalize(cross(reference, axis));
    vec3 bitangent = cross(axis, tangent);
    mat3 orientation = mat3(tangent, bitangent, axis);
    gl_Position = u_mvp * vec4(u_center + orientation * a_position * u_scale, 1.0);
    v_normal = orientation * a_normal;
}
"#,
            );
            gl.compile_shader(vertex);
            if !gl.get_shader_compile_status(vertex) {
                let error = gl.get_shader_info_log(vertex);
                gl.delete_shader(vertex);
                gl.delete_program(program);
                return Err(format!("Marcadores 3D, vertex shader: {error}"));
            }
            let fragment = match gl.create_shader(glow::FRAGMENT_SHADER) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_shader(vertex);
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.shader_source(
                fragment,
                r#"#version 330 core
in vec3 v_normal;
out vec4 out_color;
void main() {
    vec3 n = normalize(v_normal);
    vec3 light = normalize(vec3(-0.4, -0.5, 0.85));
    float diffuse = max(dot(n, light), 0.0);
    float brightness = 0.58 + 0.42 * diffuse;
    // Amarillo fuerte con sombreado por cara; material opaco.
    out_color = vec4(vec3(1.0, 0.82, 0.0) * brightness, 1.0);
}
"#,
            );
            gl.compile_shader(fragment);
            if !gl.get_shader_compile_status(fragment) {
                let error = gl.get_shader_info_log(fragment);
                gl.delete_shader(fragment);
                gl.delete_shader(vertex);
                gl.delete_program(program);
                return Err(format!("Marcadores 3D, fragment shader: {error}"));
            }
            gl.attach_shader(program, vertex);
            gl.attach_shader(program, fragment);
            gl.link_program(program);
            let linked = gl.get_program_link_status(program);
            let error = gl.get_program_info_log(program);
            gl.detach_shader(program, vertex);
            gl.detach_shader(program, fragment);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !linked {
                gl.delete_program(program);
                return Err(format!("Marcadores 3D, shader link: {error}"));
            }
            let (Some(u_mvp), Some(u_center), Some(u_scale), Some(u_axis)) = (
                gl.get_uniform_location(program, "u_mvp"),
                gl.get_uniform_location(program, "u_center"),
                gl.get_uniform_location(program, "u_scale"),
                gl.get_uniform_location(program, "u_axis"),
            ) else {
                gl.delete_program(program);
                return Err("Marcadores 3D: faltan uniforms del shader".into());
            };
            let vao = match gl.create_vertex_array() {
                Ok(vao) => vao,
                Err(error) => {
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            let vbo = match gl.create_buffer() {
                Ok(vbo) => vbo,
                Err(error) => {
                    gl.delete_vertex_array(vao);
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            let mut vertices = cone_mesh(32);
            let cone_vertices = (vertices.len() / 6) as i32;
            let sphere_offset = cone_vertices;
            vertices.extend(sphere_mesh(24, 16));
            let sphere_vertices = (vertices.len() / 6) as i32 - sphere_offset;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices.as_slice()),
                glow::STATIC_DRAW,
            );
            gl.enable_vertex_attrib_array(0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            let upload_error = gl.get_error();
            if upload_error != glow::NO_ERROR {
                gl.delete_buffer(vbo);
                gl.delete_vertex_array(vao);
                gl.delete_program(program);
                return Err(format!("Marcadores 3D, GPU 0x{upload_error:04x}"));
            }
            Ok(Self {
                program,
                vao,
                vbo,
                u_mvp,
                u_center,
                u_scale,
                u_axis,
                cone_vertices,
                sphere_offset,
                sphere_vertices,
            })
        }
    }

    /// Pintar DESPUES del STL y del bruto, pero DESPUES tambien del picking:
    /// los marcadores son indicadores, no superficies mecanizables.
    pub fn paint(&self, gl: &glow::Context, markers: &[Marker3d], camera: Camera) {
        if markers.is_empty() {
            return;
        }
        let mvp = crate::navigation::view_projection_mode(
            camera.yaw,
            camera.pitch,
            camera.zoom,
            camera.aspect,
            camera.pan,
            camera.projection,
        );
        unsafe {
            // Conservamos el depth buffer que escribio el STL; no dibujar a traves de el.
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp.to_cols_array());
            gl.uniform_1_f32(Some(&self.u_scale), MARKER_SCALE);
            gl.bind_vertex_array(Some(self.vao));
            for marker in markers {
                if !marker.position.is_finite() {
                    continue;
                }
                gl.uniform_3_f32(
                    Some(&self.u_center),
                    marker.position.x,
                    marker.position.y,
                    marker.position.z,
                );
                let axis = if marker.direction.is_finite()
                    && marker.direction.length_squared() > 1.0e-12
                {
                    marker.direction.normalize()
                } else {
                    Vec3::NEG_Z
                };
                gl.uniform_3_f32(Some(&self.u_axis), axis.x, axis.y, axis.z);
                match marker.kind {
                    MarkerKind::StartCone => gl.draw_arrays(glow::TRIANGLES, 0, self.cone_vertices),
                    MarkerKind::FinishSphere => {
                        gl.draw_arrays(glow::TRIANGLES, self.sphere_offset, self.sphere_vertices)
                    }
                }
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
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

fn add_vertex(out: &mut Vec<f32>, position: Vec3, normal: Vec3) {
    out.extend_from_slice(&[
        position.x, position.y, position.z, normal.x, normal.y, normal.z,
    ]);
}

fn add_triangle(out: &mut Vec<f32>, a: Vec3, b: Vec3, c: Vec3, normal: Vec3) {
    add_vertex(out, a, normal);
    add_vertex(out, b, normal);
    add_vertex(out, c, normal);
}

fn cone_mesh(sides: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(sides * 6 * 6);
    let apex = Vec3::new(0.0, 0.0, 2.0);
    let center = Vec3::ZERO;
    for i in 0..sides {
        let a = std::f32::consts::TAU * i as f32 / sides as f32;
        let b = std::f32::consts::TAU * (i + 1) as f32 / sides as f32;
        let p = Vec3::new(a.cos(), a.sin(), 0.0);
        let q = Vec3::new(b.cos(), b.sin(), 0.0);
        let normal = (q - p).cross(apex - p).normalize();
        add_triangle(&mut out, p, q, apex, normal);
        add_triangle(&mut out, center, q, p, Vec3::NEG_Z);
    }
    out
}

fn sphere_vertex(lat: f32, lon: f32) -> Vec3 {
    Vec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin())
}

fn sphere_mesh(sectors: usize, bands: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(sectors * bands * 6 * 6 * 2);
    for i in 0..bands {
        let low = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / bands as f32;
        let high =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i + 1) as f32 / bands as f32;
        for j in 0..sectors {
            let a = std::f32::consts::TAU * j as f32 / sectors as f32;
            let b = std::f32::consts::TAU * (j + 1) as f32 / sectors as f32;
            let p = sphere_vertex(low, a);
            let q = sphere_vertex(low, b);
            let r = sphere_vertex(high, a);
            let s = sphere_vertex(high, b);
            // Normales por vertice: aspecto esferico suave en vez de poliedro plano.
            for vertex in [p, q, r, q, s, r] {
                add_vertex(&mut out, vertex, vertex);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cone_has_real_height_and_outward_facing_surface() {
        let data = cone_mesh(32);
        assert_eq!(data.len(), 32 * 6 * 6);
        assert!(data.iter().all(|value| value.is_finite()));
        let heights: Vec<f32> = data.chunks_exact(6).map(|v| v[2]).collect();
        assert!(heights.iter().any(|&z| z == 0.0));
        assert!(heights.iter().any(|&z| z == 2.0));
        let first = Vec3::new(data[0], data[1], data[2]);
        let normal = Vec3::new(data[3], data[4], data[5]);
        assert!(first.dot(normal) > 0.0);
    }

    #[test]
    fn sphere_is_closed_three_dimensional_surface_with_normals() {
        let data = sphere_mesh(24, 16);
        assert_eq!(data.len(), 24 * 16 * 6 * 6);
        assert!(data.iter().all(|value| value.is_finite()));
        for vertex in data.chunks_exact(6) {
            let position = Vec3::from_slice(&vertex[0..3]);
            let normal = Vec3::from_slice(&vertex[3..6]);
            assert!((position.length() - 1.0).abs() < 1.0e-5);
            assert!((normal.length() - 1.0).abs() < 1.0e-5);
            assert!(position.dot(normal) > 0.99);
        }
    }
}

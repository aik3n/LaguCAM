//! LaguCAM: rejilla geometrica fija + alturas R32F desplazadas en GPU.
//! No se reduce el paso fisico del motor, ni se reconstruyen indices por seek.
//! La rejilla solo cambia al cambiar geometria del bruto o resolucion.
use crate::project::Project;
use cam_libre_visor::sparse_material::SparseMaterial;
use eframe::glow;
use glow::HasContext as _;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

const MAX_VERTICES: usize = 4_500_000;
const MAX_TEXTURE_AXIS: usize = 16_384;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct Surface {
    pub id: u64,
    pub geometry_id: u64,
    pub skirts: Vec<f32>,
    xy: Arc<Vec<f32>>,
    indices: Arc<Vec<u32>>,
    heights: Vec<f32>,
    cells: [usize; 2],
    base_z: f32,
    z_scale: f32,
    top_rel: f32,
}

impl Surface {
    /// Paredes del bruto: el borde superior sigue la misma altura que la superficie.
    /// Se actualizan con el cursor NC, pero no forman parte de la rejilla GPU fija.
    fn build_skirts(
        xy: &[f32],
        heights: &[f32],
        cells: [usize; 2],
        base_z: f32,
        z_scale: f32,
    ) -> Vec<f32> {
        let [nx, ny] = cells;
        let mut triangles = Vec::with_capacity((nx + ny) * 2 * 6 * 6);
        let mut wall = |a: usize, b: usize| {
            let top_a = [xy[a * 2], xy[a * 2 + 1], base_z + heights[a] * z_scale];
            let top_b = [xy[b * 2], xy[b * 2 + 1], base_z + heights[b] * z_scale];
            let bot_a = [top_a[0], top_a[1], base_z];
            let bot_b = [top_b[0], top_b[1], base_z];
            for pos in [top_a, top_b, bot_b, top_a, bot_b, bot_a] {
                triangles.extend_from_slice(&pos);
                triangles.extend_from_slice(&[0.32, 0.39, 0.43]);
            }
        };
        for x in 0..nx - 1 {
            wall(x, x + 1);
            wall((ny - 1) * nx + x, (ny - 1) * nx + x + 1);
        }
        for y in 0..ny - 1 {
            wall(y * nx, (y + 1) * nx);
            wall(y * nx + nx - 1, (y + 1) * nx + nx - 1);
        }
        triangles
    }

    fn samples(field: &SparseMaterial) -> Result<Vec<f32>, String> {
        let cells = field.cells();
        let stock = field.stock();
        let mut heights = Vec::with_capacity(cells[0] * cells[1]);
        for iy in 0..cells[1] {
            for ix in 0..cells[0] {
                let height = field.height_at(ix, iy).ok_or("Muestra fuera del bruto")?;
                if !height.is_finite() {
                    return Err("Altura NC no finita".into());
                }
                heights.push((height - stock.min_mm[2]) as f32);
            }
        }
        Ok(heights)
    }

    pub fn build(project: &Project, field: &SparseMaterial) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("Falta modelo STL")?;
        let stock = field.stock();
        let [nx, ny] = field.cells();
        let count = nx.checked_mul(ny).ok_or("Malla XY demasiado grande")?;
        if nx > MAX_TEXTURE_AXIS || ny > MAX_TEXTURE_AXIS {
            return Err(format!(
                "La rejilla {nx} x {ny} supera {} muestras por eje en esta version GPU. No se decima.",
                MAX_TEXTURE_AXIS
            ));
        }
        if count > MAX_VERTICES || count > u32::MAX as usize {
            return Err(format!(
                "La rejilla necesita {count} vertices (maximo {MAX_VERTICES}). No se reduce resolucion."
            ));
        }
        let index_count = (nx - 1)
            .checked_mul(ny - 1)
            .and_then(|n| n.checked_mul(6))
            .ok_or("Indices desbordados")?;
        if index_count > i32::MAX as usize {
            return Err("Indices exceden el limite de draw_elements".into());
        }
        let step = field.spacing_mm();
        let mut xy = Vec::with_capacity(count * 2);
        for iy in 0..ny {
            for ix in 0..nx {
                let p = [
                    stock.min_mm[0] + ix as f64 * step[0],
                    stock.min_mm[1] + iy as f64 * step[1],
                    stock.min_mm[2],
                ];
                let pos = model
                    .visual_point(p)
                    .ok_or("Vertice de bruto fuera de coordenadas visuales")?;
                xy.extend_from_slice(&[pos.x, pos.y]);
            }
        }
        let mut indices = Vec::with_capacity(index_count);
        for y in 0..ny - 1 {
            for x in 0..nx - 1 {
                let a = (y * nx + x) as u32;
                let b = a + 1;
                let c = a + nx as u32;
                let d = c + 1;
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        let origin = stock.min_mm;
        let below = model
            .visual_point(origin)
            .ok_or("Origen visual no valido")?;
        let top = model
            .visual_point([origin[0], origin[1], origin[2] + stock.size_mm[2]])
            .ok_or("Altura visual del bruto no valida")?;
        let scale = (top.z - below.z) / stock.size_mm[2] as f32;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("La transformacion visual Z no es creciente".into());
        }
        let heights = Self::samples(field)?;
        let skirts = Self::build_skirts(&xy, &heights, [nx, ny], below.z, scale);
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            geometry_id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            skirts,
            xy: Arc::new(xy),
            indices: Arc::new(indices),
            heights,
            cells: [nx, ny],
            base_z: below.z,
            z_scale: scale,
            top_rel: stock.size_mm[2] as f32,
        })
    }

    /// Al mover el cursor NC conserva XY e indices; solo cambia el mapa de alturas.
    pub fn updated(previous: &Self, field: &SparseMaterial) -> Result<Self, String> {
        if previous.cells != field.cells() {
            return Err("Cambio de dimensiones: es necesario reconstruir la rejilla".into());
        }
        let heights = Self::samples(field)?;
        let skirts = Self::build_skirts(
            previous.xy.as_ref(),
            &heights,
            previous.cells,
            previous.base_z,
            previous.z_scale,
        );
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            geometry_id: previous.geometry_id,
            skirts,
            xy: Arc::clone(&previous.xy),
            indices: Arc::clone(&previous.indices),
            heights,
            cells: previous.cells,
            base_z: previous.base_z,
            z_scale: previous.z_scale,
            top_rel: previous.top_rel,
        })
    }
}

pub struct GpuSurface {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    height_tex: glow::Texture,
    mvp: glow::UniformLocation,
    grid_width: glow::UniformLocation,
    base_z: glow::UniformLocation,
    z_scale: glow::UniformLocation,
    top_rel: glow::UniformLocation,
    height_sampler: glow::UniformLocation,
    geometry_id: u64,
    uploaded_id: u64,
    index_count: i32,
    cells: [usize; 2],
}

impl GpuSurface {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vs = gl.create_shader(glow::VERTEX_SHADER)?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(
                vs,
                r#"#version 330 core
layout(location=0) in vec2 a_xy;
uniform mat4 u_mvp;
uniform sampler2D u_heights;
uniform int u_grid_width;
uniform float u_base_z;
uniform float u_z_scale;
uniform float u_top_rel;
out vec3 v_pos;
out float v_cut;
void main(){
    ivec2 coord=ivec2(gl_VertexID%u_grid_width,gl_VertexID/u_grid_width);
    float height=texelFetch(u_heights,coord,0).r;
    v_pos=vec3(a_xy,u_base_z+height*u_z_scale);
    v_cut=(height<u_top_rel-0.000001)?1.0:0.0;
    gl_Position=u_mvp*vec4(v_pos,1.0);
}"#,
            );
            gl.shader_source(
                fs,
                r#"#version 330 core
in vec3 v_pos;
in float v_cut;
out vec4 fragColor;
void main(){
    vec3 dx=dFdx(v_pos),dy=dFdy(v_pos);
    vec3 n=cross(dx,dy);
    float len=length(n);
    float light=(len>1e-15)?abs(dot(n/len,normalize(vec3(0.25,-0.40,0.88)))):0.7;
    if(isnan(light)) light=0.7;
    vec3 wood=vec3(0.72,0.51,0.30),cut=vec3(0.20,0.65,0.83);
    fragColor=vec4(mix(wood,cut,clamp(v_cut,0.0,1.0))*(0.45+0.55*light),1.0);
}"#,
            );
            gl.compile_shader(vs);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(vs) || !gl.get_shader_compile_status(fs) {
                let message = format!(
                    "{} {}",
                    gl.get_shader_info_log(vs),
                    gl.get_shader_info_log(fs)
                );
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                gl.delete_program(program);
                return Err(format!("Error shader R32F: {message}"));
            }
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !gl.get_program_link_status(program) {
                let message = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("Error enlace shader R32F: {message}"));
            }
            let uniform = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("Uniform OpenGL ausente: {name}"))
            };
            let mvp = uniform("u_mvp")?;
            let grid_width = uniform("u_grid_width")?;
            let base_z = uniform("u_base_z")?;
            let z_scale = uniform("u_z_scale")?;
            let top_rel = uniform("u_top_rel")?;
            let height_sampler = uniform("u_heights")?;
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            let ebo = gl.create_buffer()?;
            let height_tex = gl.create_texture()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(height_tex));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(Self {
                program,
                vao,
                vbo,
                ebo,
                height_tex,
                mvp,
                grid_width,
                base_z,
                z_scale,
                top_rel,
                height_sampler,
                geometry_id: 0,
                uploaded_id: 0,
                index_count: 0,
                cells: [0, 0],
            })
        }
    }

    pub fn paint(&mut self, gl: &glow::Context, surface: &Surface, mvp: &glam::Mat4) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.height_tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            if self.geometry_id != surface.geometry_id {
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(surface.xy.as_ref()),
                    glow::STATIC_DRAW,
                );
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
                gl.buffer_data_u8_slice(
                    glow::ELEMENT_ARRAY_BUFFER,
                    bytemuck::cast_slice(surface.indices.as_ref()),
                    glow::STATIC_DRAW,
                );
                self.index_count = surface.indices.len() as i32;
                self.geometry_id = surface.geometry_id;
                self.cells = surface.cells;
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::R32F as i32,
                    surface.cells[0] as i32,
                    surface.cells[1] as i32,
                    0,
                    glow::RED,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&surface.heights))),
                );
                self.uploaded_id = surface.id;
            } else if self.uploaded_id != surface.id {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    surface.cells[0] as i32,
                    surface.cells[1] as i32,
                    glow::RED,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&surface.heights))),
                );
                self.uploaded_id = surface.id;
            }
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.mvp), false, &mvp.to_cols_array());
            gl.uniform_1_i32(Some(&self.grid_width), surface.cells[0] as i32);
            gl.uniform_1_f32(Some(&self.base_z), surface.base_z);
            gl.uniform_1_f32(Some(&self.z_scale), surface.z_scale);
            gl.uniform_1_f32(Some(&self.top_rel), surface.top_rel);
            gl.uniform_1_i32(Some(&self.height_sampler), 0);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_elements(glow::TRIANGLES, self.index_count, glow::UNSIGNED_INT, 0);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.height_tex);
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.ebo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_grid_has_safe_index_count() {
        assert!(MAX_VERTICES * 6 < i32::MAX as usize);
        assert!(MAX_TEXTURE_AXIS >= 2001);
    }
}

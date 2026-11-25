//! Simulacion 2.5D: bruto plano por tiles y detalle completo solo donde hay corte.
//! La resolucion de las alturas y de los tiles mecanizados NO se reduce.
//! Un solo EBO indexado para todos los tiles; no se reconstruye por fotograma.
use crate::project::Project;
use cam_libre_visor::sparse_material::{MaterialRegion, SparseMaterial};
use eframe::glow;
use glow::HasContext as _;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_SURFACE_ID: AtomicU64 = AtomicU64::new(1);
const TILE: usize = 128;
const MAX_SAMPLES: usize = 32_000_000;

pub struct Surface {
    pub id: u64,
    // Compatibilidad de la escena existente; las paredes las dibuja la GPU.
    pub skirts: Vec<f32>,
    cells: [usize; 2],
    origin_xy: [f32; 2],
    step_xy: [f32; 2],
    base_z: f32,
    z_scale: f32,
    top_rel: f32,
    // Solo se conserva la ultima version ABSOLUTA de cada tile no consumido.
    pending: Mutex<BTreeMap<(usize, usize), MaterialRegion>>,
}

impl Surface {
    pub fn build(project: &Project, field: &SparseMaterial) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("Falta modelo STL")?;
        let stock = field.stock();
        let [nx, ny] = field.cells();
        let count = nx.checked_mul(ny).ok_or("Rejilla XY fuera de rango")?;
        if nx < 2 || ny < 2 || count > MAX_SAMPLES {
            return Err(format!(
                "Rejilla de {count} alturas fuera de limite; no se reduce resolucion"
            ));
        }
        let step = field.spacing_mm();
        let origin = stock.min_mm;
        let p0 = model.visual_point(origin).ok_or("Origen visual invalido")?;
        let px = model
            .visual_point([origin[0] + step[0], origin[1], origin[2]])
            .ok_or("Paso X visual invalido")?;
        let py = model
            .visual_point([origin[0], origin[1] + step[1], origin[2]])
            .ok_or("Paso Y visual invalido")?;
        let pz = model
            .visual_point([origin[0], origin[1], origin[2] + stock.size_mm[2]])
            .ok_or("Paso Z visual invalido")?;
        let z_scale = (pz.z - p0.z) / stock.size_mm[2] as f32;
        let step_xy = [px.x - p0.x, py.y - p0.y];
        if !z_scale.is_finite()
            || z_scale <= 0.0
            || step_xy.iter().any(|step| !step.is_finite() || *step <= 0.0)
        {
            return Err("Transformacion visual incompatible con la rejilla XY".into());
        }
        Ok(Self {
            id: NEXT_SURFACE_ID.fetch_add(1, Ordering::Relaxed),
            skirts: Vec::new(),
            cells: [nx, ny],
            origin_xy: [p0.x, p0.y],
            step_xy,
            base_z: p0.z,
            z_scale,
            top_rel: stock.size_mm[2] as f32,
            pending: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn queue_regions(&self, regions: Vec<MaterialRegion>) {
        if regions.is_empty() {
            return;
        }
        if let Ok(mut queue) = self.pending.lock() {
            for region in regions {
                queue.insert((region.x, region.y), region);
            }
        }
    }

    fn take_regions(&self) -> Vec<MaterialRegion> {
        if let Ok(mut queue) = self.pending.lock() {
            std::mem::take(&mut *queue).into_values().collect()
        } else {
            Vec::new()
        }
    }
}

pub struct GpuSurface {
    program: glow::Program,
    vao: glow::VertexArray,
    ebo: glow::Buffer,
    heights: glow::Texture,
    mask: glow::Texture,
    u_mvp: glow::UniformLocation,
    u_heights: glow::UniformLocation,
    u_mask: glow::UniformLocation,
    u_grid: glow::UniformLocation,
    u_tile_grid: glow::UniformLocation,
    u_tile_origin: glow::UniformLocation,
    u_origin_xy: glow::UniformLocation,
    u_step_xy: glow::UniformLocation,
    u_base_z: glow::UniformLocation,
    u_scale_z: glow::UniformLocation,
    u_top_rel: glow::UniformLocation,
    u_mode: glow::UniformLocation,
    uploaded_id: u64,
    tile_grid: [usize; 2],
    detail_tiles: Vec<u8>,
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
uniform mat4 u_mvp;
uniform sampler2D u_heights;
uniform sampler2D u_mask;
uniform ivec2 u_grid;
uniform ivec2 u_tile_grid;
uniform ivec2 u_tile_origin;
uniform vec2 u_origin_xy;
uniform vec2 u_step_xy;
uniform float u_base_z;
uniform float u_scale_z;
uniform float u_top_rel;
uniform int u_mode;
out vec3 v_pos;
out float v_cut;
out float v_wall;

void main(){
    int vid = gl_VertexID % 6;
    int dx = (vid == 1 || vid == 2 || vid == 4) ? 1 : 0;
    int dy = (vid == 2 || vid == 4 || vid == 5) ? 1 : 0;
    ivec2 pixel;
    float z;
    if (u_mode == 0) {
        // Un unico cuadrilatero por tile de bruto sin mecanizar.
        int tx = gl_InstanceID % u_tile_grid.x;
        int ty = gl_InstanceID / u_tile_grid.x;
        if (texelFetch(u_mask, ivec2(tx,ty), 0).r > 0.5) {
            v_pos = vec3(0.0); v_cut = 0.0; v_wall = 0.0;
            gl_Position = vec4(3.0, 3.0, 3.0, 1.0);
            return;
        }
        ivec2 a = ivec2(tx,ty)*128;
        ivec2 b = min(a + ivec2(128), u_grid - ivec2(1));
        pixel = ivec2(dx == 1 ? b.x : a.x, dy == 1 ? b.y : a.y);
        z = u_top_rel;
    } else if (u_mode == 1) {
        // Indices compartidos para 129 x 129 vertices. Ningun detalle eliminado.
        pixel = u_tile_origin + ivec2(gl_VertexID % 129, gl_VertexID / 129);
        pixel = min(pixel, u_grid - ivec2(1));
        z = texelFetch(u_heights, pixel, 0).r;
    } else {
        // Paredes laterales continuas del bruto.
        int nx = u_grid.x - 1;
        int ny = u_grid.y - 1;
        int edge = gl_InstanceID;
        ivec2 a;
        ivec2 b;
        if (edge < nx) {
            a = ivec2(edge, 0); b = a + ivec2(1, 0);
        } else if (edge < nx + ny) {
            a = ivec2(nx, edge - nx); b = a + ivec2(0, 1);
        } else if (edge < 2 * nx + ny) {
            a = ivec2(edge - (nx + ny), ny); b = a + ivec2(1, 0);
        } else {
            a = ivec2(0, edge - (2 * nx + ny)); b = a + ivec2(0, 1);
        }
        pixel = (vid == 1 || vid == 2 || vid == 4) ? b : a;
        z = (vid == 2 || vid == 4 || vid == 5)
            ? 0.0 : texelFetch(u_heights, pixel, 0).r;
    }
    v_pos = vec3(u_origin_xy + vec2(pixel)*u_step_xy,
                 u_base_z + z*u_scale_z);
    v_wall = u_mode == 2 ? 1.0 : 0.0;
    v_cut = z < u_top_rel - 0.000001 ? 1.0 : 0.0;
    gl_Position = u_mvp * vec4(v_pos, 1.0);
}"#,
            );
            gl.shader_source(
                fs,
                r#"#version 330 core
in vec3 v_pos;
in float v_cut;
in float v_wall;
out vec4 fragColor;
void main(){
    vec3 cross_normal = cross(dFdx(v_pos), dFdy(v_pos));
    float len = length(cross_normal);
    vec3 normal = len > 1.e-12 ? cross_normal / len : vec3(0.0,0.0,1.0);
    float light = abs(dot(normal, normalize(vec3(0.25,-0.40,0.88))));
    if (isnan(light)) light = 0.7;
    vec3 wood = vec3(0.72,0.51,0.30);
    vec3 cut = vec3(0.20,0.65,0.83);
    vec3 color = v_wall > 0.5 ? vec3(0.32,0.39,0.43)
        : mix(wood, cut, clamp(v_cut, 0.0, 1.0));
    fragColor = vec4(color * (0.45 + 0.55*light),1.0);
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
                return Err(format!("Shader material GPU: {message}"));
            }
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !gl.get_program_link_status(program) {
                let message = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("Enlace shader de material: {message}"));
            }
            let uniform = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("Falta uniforme GPU: {name}"))
            };
            let u_mvp = uniform("u_mvp")?;
            let u_heights = uniform("u_heights")?;
            let u_mask = uniform("u_mask")?;
            let u_grid = uniform("u_grid")?;
            let u_tile_grid = uniform("u_tile_grid")?;
            let u_tile_origin = uniform("u_tile_origin")?;
            let u_origin_xy = uniform("u_origin_xy")?;
            let u_step_xy = uniform("u_step_xy")?;
            let u_base_z = uniform("u_base_z")?;
            let u_scale_z = uniform("u_scale_z")?;
            let u_top_rel = uniform("u_top_rel")?;
            let u_mode = uniform("u_mode")?;
            let vao = gl.create_vertex_array()?;
            let ebo = gl.create_buffer()?;
            let heights = gl.create_texture()?;
            let mask = gl.create_texture()?;

            let mut indices = Vec::<u32>::with_capacity(TILE * TILE * 6);
            for iy in 0..TILE {
                for ix in 0..TILE {
                    let a = (iy * (TILE + 1) + ix) as u32;
                    let b = a + 1;
                    let c = a + (TILE + 1) as u32;
                    let d = c + 1;
                    indices.extend_from_slice(&[a, b, d, a, d, c]);
                }
            }
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&indices),
                glow::STATIC_DRAW,
            );
            gl.bind_vertex_array(None);

            for (unit, tex) in [(glow::TEXTURE0, heights), (glow::TEXTURE1, mask)] {
                gl.active_texture(unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
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
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            Ok(Self {
                program,
                vao,
                ebo,
                heights,
                mask,
                u_mvp,
                u_heights,
                u_mask,
                u_grid,
                u_tile_grid,
                u_tile_origin,
                u_origin_xy,
                u_step_xy,
                u_base_z,
                u_scale_z,
                u_top_rel,
                u_mode,
                uploaded_id: 0,
                tile_grid: [0, 0],
                detail_tiles: Vec::new(),
            })
        }
    }

    pub fn paint(&mut self, gl: &glow::Context, surface: &Surface, mvp: &glam::Mat4) {
        unsafe {
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.heights));
            if self.uploaded_id != surface.id {
                let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
                if surface.cells[0] as i32 > max_tex || surface.cells[1] as i32 > max_tex {
                    eprintln!("LaguCAM: alturas exceden limite de textura OpenGL {max_tex}");
                    return;
                }
                let initial = vec![surface.top_rel; surface.cells[0] * surface.cells[1]];
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::R32F as i32,
                    surface.cells[0] as i32,
                    surface.cells[1] as i32,
                    0,
                    glow::RED,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&initial))),
                );
                self.tile_grid = [
                    (surface.cells[0] - 2) / TILE + 1,
                    (surface.cells[1] - 2) / TILE + 1,
                ];
                self.detail_tiles = vec![0u8; self.tile_grid[0] * self.tile_grid[1]];
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.mask));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::R8 as i32,
                    self.tile_grid[0] as i32,
                    self.tile_grid[1] as i32,
                    0,
                    glow::RED,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&self.detail_tiles)),
                );
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.heights));
                self.uploaded_id = surface.id;
            }
            let mut new_tiles = false;
            for region in surface.take_regions() {
                if region.width == 0
                    || region.height == 0
                    || region.x + region.width > surface.cells[0]
                    || region.y + region.height > surface.cells[1]
                    || region.values.len() != region.width * region.height
                {
                    continue;
                }
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    region.x as i32,
                    region.y as i32,
                    region.width as i32,
                    region.height as i32,
                    glow::RED,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&region.values))),
                );
                // La muestra de la frontera tambien pertenece a la celda vecina.
                let tx0 = (region.x.saturating_sub(1) / TILE).min(self.tile_grid[0] - 1);
                let ty0 = (region.y.saturating_sub(1) / TILE).min(self.tile_grid[1] - 1);
                let tx1 = ((region.x + region.width - 1) / TILE).min(self.tile_grid[0] - 1);
                let ty1 = ((region.y + region.height - 1) / TILE).min(self.tile_grid[1] - 1);
                for ty in ty0..=ty1 {
                    for tx in tx0..=tx1 {
                        let idx = ty * self.tile_grid[0] + tx;
                        if self.detail_tiles[idx] == 0 {
                            self.detail_tiles[idx] = 255;
                            new_tiles = true;
                        }
                    }
                }
            }
            if new_tiles {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.mask));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    self.tile_grid[0] as i32,
                    self.tile_grid[1] as i32,
                    glow::RED,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&self.detail_tiles)),
                );
            }
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp.to_cols_array());
            gl.uniform_1_i32(Some(&self.u_heights), 0);
            gl.uniform_1_i32(Some(&self.u_mask), 1);
            gl.uniform_2_i32(
                Some(&self.u_grid),
                surface.cells[0] as i32,
                surface.cells[1] as i32,
            );
            gl.uniform_2_i32(
                Some(&self.u_tile_grid),
                self.tile_grid[0] as i32,
                self.tile_grid[1] as i32,
            );
            gl.uniform_2_f32(
                Some(&self.u_origin_xy),
                surface.origin_xy[0],
                surface.origin_xy[1],
            );
            gl.uniform_2_f32(
                Some(&self.u_step_xy),
                surface.step_xy[0],
                surface.step_xy[1],
            );
            gl.uniform_1_f32(Some(&self.u_base_z), surface.base_z);
            gl.uniform_1_f32(Some(&self.u_scale_z), surface.z_scale);
            gl.uniform_1_f32(Some(&self.u_top_rel), surface.top_rel);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.heights));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.mask));
            gl.bind_vertex_array(Some(self.vao));

            gl.uniform_1_i32(Some(&self.u_mode), 0);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                6,
                (self.tile_grid[0] * self.tile_grid[1]) as i32,
            );

            gl.uniform_1_i32(Some(&self.u_mode), 1);
            for ty in 0..self.tile_grid[1] {
                for tx in 0..self.tile_grid[0] {
                    if self.detail_tiles[ty * self.tile_grid[0] + tx] == 0 {
                        continue;
                    }
                    gl.uniform_2_i32(
                        Some(&self.u_tile_origin),
                        (tx * TILE) as i32,
                        (ty * TILE) as i32,
                    );
                    gl.draw_elements(
                        glow::TRIANGLES,
                        (TILE * TILE * 6) as i32,
                        glow::UNSIGNED_INT,
                        0,
                    );
                }
            }

            gl.uniform_1_i32(Some(&self.u_mode), 2);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                6,
                (2 * ((surface.cells[0] - 1) + (surface.cells[1] - 1))) as i32,
            );
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.mask);
            gl.delete_texture(self.heights);
            gl.delete_buffer(self.ebo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

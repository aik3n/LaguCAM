//! Renderer de material 2.5D: rejilla implicita, alturas R32F y parches GPU.
//! La CPU no construye mallas ni recorre toda la superficie durante un frame.
use crate::project::Project;
use cam_libre_visor::sparse_material::{MaterialRegion, SparseMaterial};
use eframe::glow;
use glow::HasContext as _;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

static NEXT_SURFACE_ID: AtomicU64 = AtomicU64::new(1);
const MAX_SAMPLES: usize = 32_000_000;

pub struct Surface {
    pub id: u64,
    // La escena antigua consume este campo. Las paredes se dibujan en GPU.
    pub skirts: Vec<f32>,
    cells: [usize; 2],
    origin_xy: [f32; 2],
    step_xy: [f32; 2],
    base_z: f32,
    z_scale: f32,
    top_rel: f32,
    pending: Mutex<Vec<MaterialRegion>>,
}

impl Surface {
    pub fn build(project: &Project, field: &SparseMaterial) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("Falta modelo STL")?;
        let stock = field.stock();
        let [nx, ny] = field.cells();
        let samples = nx.checked_mul(ny).ok_or("Dimensiones XY fuera de rango")?;
        if nx < 2 || ny < 2 || samples > MAX_SAMPLES {
            return Err(format!(
                "Rejilla de {samples} muestras fuera de limite GPU; no se reduce resolucion"
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
            || step_xy.iter().any(|s| !s.is_finite() || *s <= 0.0)
        {
            return Err("La transformacion visual XYZ no es compatible con la rejilla".into());
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
            pending: Mutex::new(Vec::new()),
        })
    }

    pub fn queue_regions(&self, mut regions: Vec<MaterialRegion>) {
        if regions.is_empty() {
            return;
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.append(&mut regions);
        }
    }

    fn take_regions(&self) -> Vec<MaterialRegion> {
        if let Ok(mut pending) = self.pending.lock() {
            std::mem::take(&mut *pending)
        } else {
            Vec::new()
        }
    }
}

pub struct GpuSurface {
    program: glow::Program,
    vao: glow::VertexArray,
    texture: glow::Texture,
    u_mvp: glow::UniformLocation,
    u_heights: glow::UniformLocation,
    u_grid: glow::UniformLocation,
    u_xy_origin: glow::UniformLocation,
    u_xy_step: glow::UniformLocation,
    u_base_z: glow::UniformLocation,
    u_z_scale: glow::UniformLocation,
    u_top_rel: glow::UniformLocation,
    u_walls: glow::UniformLocation,
    uploaded_id: u64,
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
uniform ivec2 u_grid;
uniform vec2 u_xy_origin;
uniform vec2 u_xy_step;
uniform float u_base_z;
uniform float u_z_scale;
uniform float u_top_rel;
uniform int u_walls;
out vec3 v_pos;
out float v_cut;
out float v_wall;

void main(){
    int vid = gl_VertexID % 6;
    int dx = (vid == 1 || vid == 2 || vid == 4) ? 1 : 0;
    int dy = (vid == 2 || vid == 4 || vid == 5) ? 1 : 0;
    ivec2 pixel;
    float z;
    if (u_walls == 0) {
        int width = u_grid.x - 1;
        int cell_x = gl_InstanceID % width;
        int cell_y = gl_InstanceID / width;
        pixel = ivec2(cell_x + dx, cell_y + dy);
        z = texelFetch(u_heights, pixel, 0).r;
    } else {
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
        // Dos triangulos por tramo: (aTop,bTop,bBottom,aTop,bBottom,aBottom)
        pixel = (vid == 1 || vid == 2 || vid == 4) ? b : a;
        z = (vid == 2 || vid == 4 || vid == 5) ? 0.0 : texelFetch(u_heights, pixel, 0).r;
    }
    v_pos = vec3(u_xy_origin + vec2(pixel) * u_xy_step,
                 u_base_z + z * u_z_scale);
    v_wall = float(u_walls);
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
    vec3 color = v_wall > 0.5 ? vec3(0.32,0.39,0.43) : mix(wood, cut, clamp(v_cut, 0.0, 1.0));
    fragColor = vec4(color * (0.45 + 0.55 * light),1.0);
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
                return Err(format!("Shader mapa de alturas: {message}"));
            }
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !gl.get_program_link_status(program) {
                let message = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("Enlace shader mapa de alturas: {message}"));
            }
            let uniform = |name: &str| {
                gl.get_uniform_location(program, name)
                    .ok_or_else(|| format!("Falta uniforme {name}"))
            };
            let u_mvp = uniform("u_mvp")?;
            let u_heights = uniform("u_heights")?;
            let u_grid = uniform("u_grid")?;
            let u_xy_origin = uniform("u_xy_origin")?;
            let u_xy_step = uniform("u_xy_step")?;
            let u_base_z = uniform("u_base_z")?;
            let u_z_scale = uniform("u_z_scale")?;
            let u_top_rel = uniform("u_top_rel")?;
            let u_walls = uniform("u_walls")?;
            let vao = gl.create_vertex_array()?;
            let texture = gl.create_texture()?;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
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
                texture,
                u_mvp,
                u_heights,
                u_grid,
                u_xy_origin,
                u_xy_step,
                u_base_z,
                u_z_scale,
                u_top_rel,
                u_walls,
                uploaded_id: 0,
            })
        }
    }

    pub fn paint(&mut self, gl: &glow::Context, surface: &Surface, mvp: &glam::Mat4) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            if self.uploaded_id != surface.id {
                let max_tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
                if surface.cells[0] as i32 > max_tex || surface.cells[1] as i32 > max_tex {
                    eprintln!("LaguCAM: rejilla superior al limite de textura GPU ({max_tex})");
                    return;
                }
                let pixels = surface.cells[0] * surface.cells[1];
                let initial = vec![surface.top_rel; pixels];
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
                self.uploaded_id = surface.id;
            }
            // Cada lote es el resultado de un estado NC completado y ordenado.
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
            }
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, &mvp.to_cols_array());
            gl.uniform_1_i32(Some(&self.u_heights), 0);
            gl.uniform_2_i32(
                Some(&self.u_grid),
                surface.cells[0] as i32,
                surface.cells[1] as i32,
            );
            gl.uniform_2_f32(
                Some(&self.u_xy_origin),
                surface.origin_xy[0],
                surface.origin_xy[1],
            );
            gl.uniform_2_f32(
                Some(&self.u_xy_step),
                surface.step_xy[0],
                surface.step_xy[1],
            );
            gl.uniform_1_f32(Some(&self.u_base_z), surface.base_z);
            gl.uniform_1_f32(Some(&self.u_z_scale), surface.z_scale);
            gl.uniform_1_f32(Some(&self.u_top_rel), surface.top_rel);
            gl.bind_vertex_array(Some(self.vao));
            gl.uniform_1_i32(Some(&self.u_walls), 0);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                6,
                ((surface.cells[0] - 1) * (surface.cells[1] - 1)) as i32,
            );
            gl.uniform_1_i32(Some(&self.u_walls), 1);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                6,
                (2 * ((surface.cells[0] - 1) + (surface.cells[1] - 1))) as i32,
            );
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.texture);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

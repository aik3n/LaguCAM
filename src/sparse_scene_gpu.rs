//! Superficie completa indexada del material disperso, sin decimacion espacial.
//! La geometria se construye una vez por cambio de posicion NC; la camara solo
//! vuelve a dibujar buffers GPU existentes. Iluminacion por cara en el fragment shader.
use crate::project::Project;
use cam_libre_visor::sparse_material::SparseMaterial;
use eframe::glow;
use glow::HasContext as _;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_VERTICES: usize = 4_500_000;
static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

pub struct Surface {
    pub id: u64,
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub skirts: Vec<f32>,
}

impl Surface {
    pub fn build(project: &Project, field: &SparseMaterial) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("Falta modelo STL para convertir coordenadas")?;
        let [nx, ny] = field.cells();
        let count = nx.checked_mul(ny).ok_or("Desbordamiento de malla")?;
        if count > MAX_VERTICES {
            return Err(format!(
                "La malla a resolucion real necesita {count} vertices; limite GPU {}. No se reduce la resolucion.",
                MAX_VERTICES
            ));
        }
        if count > u32::MAX as usize { return Err("Indices GPU fuera de u32".into()); }
        let faces = (nx - 1).checked_mul(ny - 1)
            .and_then(|n| n.checked_mul(6)).ok_or("Desbordamiento de indices")?;
        let stock = field.stock();
        let step = field.spacing_mm();
        let top = stock.min_mm[2] + stock.size_mm[2];
        let mut vertices = Vec::with_capacity(count * 6);
        let mut indices = Vec::with_capacity(faces);
        for iy in 0..ny {
            for ix in 0..nx {
                let height = field.height_at(ix, iy).ok_or("Altura XY fuera de malla")?;
                let p = [
                    stock.min_mm[0] + ix as f64 * step[0],
                    stock.min_mm[1] + iy as f64 * step[1],
                    height,
                ];
                let position = model.visual_point(p)
                    .ok_or("No se pudo normalizar un vertice del material")?;
                vertices.extend_from_slice(&position.to_array());
                let color = if height < top - 0.000001 {
                    [0.20, 0.65, 0.83]
                } else {
                    [0.72, 0.51, 0.30]
                };
                vertices.extend_from_slice(&color);
            }
        }
        for y in 0..ny - 1 {
            for x in 0..nx - 1 {
                let a = (y * nx + x) as u32;
                let b = a + 1;
                let c = a + nx as u32;
                let d = c + 1;
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        // Faldones verticales del bruto, a la misma escala que su superficie.
        // Sus vertices superiores coinciden exactamente con los de la malla indexada.
        let mut skirts = Vec::new();
        let mut wall = |a: usize, b: usize| -> Result<(), String> {
            let top_a = &vertices[a * 6..a * 6 + 3];
            let top_b = &vertices[b * 6..b * 6 + 3];
            let bottom_at = |index: usize| -> Result<[f32; 3], String> {
                let ix = index % nx;
                let iy = index / nx;
                let p = [
                    stock.min_mm[0] + ix as f64 * step[0],
                    stock.min_mm[1] + iy as f64 * step[1],
                    stock.min_mm[2],
                ];
                model.visual_point(p).map(|p| p.to_array())
                    .ok_or_else(|| "Faldon fuera de coordenadas visuales".to_string())
            };
            let bot_a = bottom_at(a)?;
            let bot_b = bottom_at(b)?;
            for pos in [top_a, top_b, &bot_b, top_a, &bot_b, &bot_a] {
                skirts.extend_from_slice(pos);
                skirts.extend_from_slice(&[0.32, 0.39, 0.43]);
            }
            Ok(())
        };
        for x in 0..nx - 1 {
            wall(x, x + 1)?;
            wall((ny - 1) * nx + x, (ny - 1) * nx + x + 1)?;
        }
        for y in 0..ny - 1 {
            wall(y * nx, (y + 1) * nx)?;
            wall(y * nx + nx - 1, (y + 1) * nx + nx - 1)?;
        }
        Ok(Self {id: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed), vertices, indices, skirts})
    }
}

pub struct GpuSurface {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    mvp: glow::UniformLocation,
    uploaded_id: u64,
    index_count: i32,
}

impl GpuSurface {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let vs = gl.create_shader(glow::VERTEX_SHADER)?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER)?;
            gl.shader_source(vs, r#"#version 330 core
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_col;
uniform mat4 u_mvp;
out vec3 v_pos;
out vec3 v_col;
void main(){v_pos=a_pos;v_col=a_col;gl_Position=u_mvp*vec4(a_pos,1.0);}"#);
            gl.shader_source(fs, r#"#version 330 core
in vec3 v_pos;
in vec3 v_col;
out vec4 fragColor;
void main(){
    vec3 dx=dFdx(v_pos),dy=dFdy(v_pos);
    vec3 normal=normalize(cross(dx,dy));
    float l=abs(dot(normal,normalize(vec3(0.25,-0.40,0.88))));
    if(isnan(l)) l=0.70;
    fragColor=vec4(v_col*(0.45+0.55*l),1.0);
}"#);
            gl.compile_shader(vs);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(vs) || !gl.get_shader_compile_status(fs) {
                let err = format!("{} {}", gl.get_shader_info_log(vs), gl.get_shader_info_log(fs));
                gl.delete_shader(vs);gl.delete_shader(fs);gl.delete_program(program);
                return Err(format!("Shader material disperso: {err}"));
            }
            gl.attach_shader(program,vs);gl.attach_shader(program,fs);gl.link_program(program);
            gl.delete_shader(vs);gl.delete_shader(fs);
            if !gl.get_program_link_status(program) {
                let err=gl.get_program_info_log(program);gl.delete_program(program);
                return Err(format!("Enlazando shader de material: {err}"));
            }
            let mvp=gl.get_uniform_location(program,"u_mvp")
                .ok_or("Falta uniforme de camara en shader de material")?;
            let vao=gl.create_vertex_array()?;
            let vbo=gl.create_buffer()?;
            let ebo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0,3,glow::FLOAT,false,24,0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1,3,glow::FLOAT,false,24,12);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER,None);
            Ok(Self{program,vao,vbo,ebo,mvp,uploaded_id:0,index_count:0})
        }
    }

    pub fn paint(&mut self, gl:&glow::Context, surface:&Surface, mvp:&glam::Mat4) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.mvp),false,&mvp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            if self.uploaded_id != surface.id {
                gl.bind_buffer(glow::ARRAY_BUFFER,Some(self.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,bytemuck::cast_slice(&surface.vertices),glow::DYNAMIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(self.ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,bytemuck::cast_slice(&surface.indices),glow::DYNAMIC_DRAW);
                self.index_count=surface.indices.len() as i32;
                self.uploaded_id=surface.id;
            }
            gl.draw_elements(glow::TRIANGLES,self.index_count,glow::UNSIGNED_INT,0);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }

    pub fn destroy(self, gl:&glow::Context) {
        unsafe {
            gl.delete_buffer(self.vbo);gl.delete_buffer(self.ebo);
            gl.delete_vertex_array(self.vao);gl.delete_program(self.program);
        }
    }
}
//! Consulta de superficie sobre la profundidad de la escena renderizada.
//! No depende del formato original ni retiene triángulos en RAM.
//! Solo se ejecuta cuando el usuario pide un pivote (clic simple).
use eframe::glow;
use glam::{Mat4, Vec3};
use glow::HasContext as _;

#[derive(Clone, Copy)]
pub struct PixelViewport {
    pub left: i32,
    pub bottom: i32,
    pub width: i32,
    pub height: i32,
}

impl PixelViewport {
    fn contains(&self, x: i32, y: i32) -> bool {
        self.width > 0
            && self.height > 0
            && x >= self.left
            && y >= self.bottom
            && x < self.left.saturating_add(self.width)
            && y < self.bottom.saturating_add(self.height)
    }
}

/// `x,y` están en coordenadas físicas del framebuffer con origen inferior izquierdo.
/// El framebuffer `source` es el utilizado para dibujar la escena, sea MSAA o no.
/// El blit 1x1 a un FBO sin MSAA evita el error de leer directamente muestras múltiples.
pub fn pick_visible_surface(
    gl: &glow::Context,
    source: Option<glow::Framebuffer>,
    viewport: PixelViewport,
    x: i32,
    y: i32,
    mvp: Mat4,
) -> Result<Option<Vec3>, String> {
    if !viewport.contains(x, y) {
        return Ok(None);
    }
    // La creación y eliminación del FBO ocurren en el callback con contexto GL activo.
    unsafe {
        let fbo = gl.create_framebuffer()?;
        let rb = match gl.create_renderbuffer() {
            Ok(rb) => rb,
            Err(error) => {
                gl.delete_framebuffer(fbo);
                return Err(error);
            }
        };
        let old_pack_buffer = gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
        gl.disable(glow::SCISSOR_TEST);
        let result = (|| -> Result<Option<Vec3>, String> {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, 1, 1);
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(rb),
            );
            gl.draw_buffer(glow::NONE);
            gl.read_buffer(glow::NONE);
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err("Framebuffer auxiliar de profundidad no disponible".into());
            }
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, source);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
            gl.blit_framebuffer(
                x,
                y,
                x + 1,
                y + 1,
                0,
                0,
                1,
                1,
                glow::DEPTH_BUFFER_BIT,
                glow::NEAREST,
            );
            if gl.get_error() != glow::NO_ERROR {
                return Err("El controlador no permite resolver la profundidad del visor".into());
            }
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            let mut bytes = [0_u8; 4];
            gl.read_pixels(
                0,
                0,
                1,
                1,
                glow::DEPTH_COMPONENT,
                glow::FLOAT,
                glow::PixelPackData::Slice(Some(&mut bytes)),
            );
            if gl.get_error() != glow::NO_ERROR {
                return Err("No se pudo leer la profundidad del punto seleccionado".into());
            }
            let depth = f32::from_ne_bytes(bytes);
            Ok(crate::navigation::unproject_depth(
                (x - viewport.left) as f32 + 0.5,
                (y - viewport.bottom) as f32 + 0.5,
                viewport.width,
                viewport.height,
                depth,
                mvp,
            ))
        })();
        // Restituir el estado del framebuffer incluso si falla la lectura.
        gl.bind_framebuffer(glow::FRAMEBUFFER, source);
        gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, old_pack_buffer);
        gl.enable(glow::SCISSOR_TEST);
        gl.delete_renderbuffer(rb);
        gl.delete_framebuffer(fbo);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_coordinates_are_consistent() {
        let v = PixelViewport {
            left: 100,
            bottom: 20,
            width: 500,
            height: 400,
        };
        assert!(v.contains(100, 20));
        assert!(v.contains(599, 419));
        assert!(!v.contains(600, 120));
        assert!(!v.contains(100, 420));
        assert!(!v.contains(99, 100));
        assert!(!v.contains(200, 19));
    }
}

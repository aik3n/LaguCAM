//! Iconos Lucide para LaguCAM. Assets rasterizados desde SVG oficiales a 48px,
//! con alfa y tinta dinamica; sin fuentes Unicode ni cargadores adicionales.
//! Fuentes: https://github.com/lucide-icons/lucide (licencia ISC).
use eframe::egui;

const ICON_PIXELS: [usize; 2] = [48, 48];
const EYE: &[u8] = include_bytes!("../assets/lucide/eye.rgba");
const EYE_OFF: &[u8] = include_bytes!("../assets/lucide/eye-off.rgba");

fn texture(ui: &egui::Ui, visible: bool) -> egui::TextureHandle {
    let key = if visible { "eye" } else { "eye-off" };
    let id = ui.make_persistent_id(("lagucam", "lucide", key));
    // No llamar a load_texture dentro del bloqueo de Context::data_mut:
    // puede producir un interbloqueo por reentrada al construir el arbol de operaciones.
    let cached = ui
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id));
    if let Some(texture) = cached {
        return texture;
    }
    let pixels = if visible { EYE } else { EYE_OFF };
    let image = egui::ColorImage::from_rgba_unmultiplied(ICON_PIXELS, pixels);
    let texture = ui.ctx().load_texture(
        format!("lagucam-lucide-{key}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

/// Visibilidad: ojo verde / ojo tachado gris. La decision pertenece a Operation.
pub fn visibility_button(ui: &mut egui::Ui, visible: bool) -> egui::Response {
    let color = if visible {
        egui::Color32::from_rgb(49, 183, 106)
    } else {
        egui::Color32::from_rgb(133, 139, 149)
    };
    let image = egui::Image::new((texture(ui, visible).id(), egui::vec2(18.0, 18.0))).tint(color);
    ui.add(
        egui::Button::image(image)
            .frame(false)
            .min_size(egui::vec2(28.0, 24.0))
            .selected(visible),
    )
    .on_hover_text(if visible {
        "Ocultar trayectoria 3D"
    } else {
        "Mostrar trayectoria 3D"
    })
}

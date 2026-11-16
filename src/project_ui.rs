//! UI only: editable properties commit atomically after data-level validation.
use crate::project::{
    Project,
    display::{StockColor, StockMode},
    operations::{OperationKind, OperationStatus},
    origin::{OriginPoint, OriginReference},
    selection::Selection,
};
use eframe::egui;

#[derive(Clone, Copy)]
enum TreeIcon {
    Project,
    Model,
    Stock,
    Origin,
    Operations,
    Operation,
}

fn paint_tree_icon(ui: &egui::Ui, rect: egui::Rect, icon: TreeIcon, header: bool, selected: bool) {
    let center = if header {
        // La cabecera coloca el icono en un rectangulo propio, sin espacios de texto.
        rect.center()
    } else {
        egui::pos2(rect.left() + 9.0, rect.center().y)
    };
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let stroke = egui::Stroke::new(1.4, color);
    let at = |x: f32, y: f32| center + egui::vec2(x, y);
    let line = |a: (f32, f32), b: (f32, f32)| {
        ui.painter()
            .line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    };
    match icon {
        TreeIcon::Project => {
            // Carpeta: identificador del proyecto.
            line((-6.0, -3.0), (-2.0, -3.0));
            line((-2.0, -3.0), (0.0, -1.0));
            line((0.0, -1.0), (6.0, -1.0));
            line((6.0, -1.0), (5.0, 5.0));
            line((5.0, 5.0), (-5.0, 5.0));
            line((-5.0, 5.0), (-6.0, -3.0));
        }
        TreeIcon::Model => {
            // Cubo isometrico: modelo 3D.
            line((-6.0, -3.0), (0.0, -6.0));
            line((0.0, -6.0), (6.0, -3.0));
            line((6.0, -3.0), (0.0, 0.0));
            line((0.0, 0.0), (-6.0, -3.0));
            line((-6.0, -3.0), (-6.0, 3.0));
            line((-6.0, 3.0), (0.0, 6.0));
            line((0.0, 6.0), (6.0, 3.0));
            line((6.0, 3.0), (6.0, -3.0));
            line((0.0, 0.0), (0.0, 6.0));
        }
        TreeIcon::Stock => {
            // Bloque rectangular: material en bruto.
            line((-6.0, -5.0), (6.0, -5.0));
            line((6.0, -5.0), (6.0, 5.0));
            line((6.0, 5.0), (-6.0, 5.0));
            line((-6.0, 5.0), (-6.0, -5.0));
            line((-6.0, -1.0), (6.0, -1.0));
            line((-2.0, -5.0), (-2.0, -1.0));
        }
        TreeIcon::Origin => {
            // Diana de origen CNC.
            ui.painter().circle_stroke(center, 3.2, stroke);
            line((-6.0, 0.0), (-3.5, 0.0));
            line((3.5, 0.0), (6.0, 0.0));
            line((0.0, -6.0), (0.0, -3.5));
            line((0.0, 3.5), (0.0, 6.0));
        }
        TreeIcon::Operations => {
            // Lista de operaciones.
            for y in [-4.0, 0.0, 4.0] {
                ui.painter().circle_filled(at(-5.0, y), 1.0, color);
                line((-2.0, y), (6.0, y));
            }
        }
        TreeIcon::Operation => {
            // Recorrido estilizado; no representa trayectoria CNC validada.
            line((-5.0, 4.0), (-1.0, 4.0));
            line((-1.0, 4.0), (-1.0, -3.0));
            line((-1.0, -3.0), (5.0, -3.0));
            ui.painter().circle_filled(at(-5.0, 4.0), 1.2, color);
            ui.painter().circle_stroke(at(5.0, -3.0), 1.8, stroke);
        }
    }
}

fn icon_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    icon: TreeIcon,
) -> egui::Response {
    // El icono va dentro del mismo widget seleccionable: conserva hitbox y doble clic.
    let response = ui.selectable_label(selected, format!("     {label}"));
    paint_tree_icon(ui, response.rect, icon, false, selected);
    response
}
// Cabeceras: 16 puntos de ancho para el icono de 12 puntos.
// El dibujo deja 2 puntos libres dentro del rectangulo por la derecha:
// 6 puntos de espaciado de egui + 2 internos = 8 puntos visuales.
// El espaciado no depende de la fuente, del tema ni de espacios en el texto.
fn tree_header_label(ui: &mut egui::Ui, icon: TreeIcon, title: &str) -> bool {
    ui.spacing_mut().item_spacing.x = 6.0;
    let height = ui.spacing().interact_size.y;
    let (icon_rect, icon_response) =
        ui.allocate_exact_size(egui::vec2(16.0, height), egui::Sense::click());
    paint_tree_icon(ui, icon_rect, icon, true, false);
    let title_response = ui.add(
        egui::Label::new(egui::RichText::new(title).text_style(egui::TextStyle::Button))
            .sense(egui::Sense::click()),
    );
    icon_response.clicked() || title_response.clicked()
}

#[derive(Default)]
pub struct OperationTreeActions {
    pub edit: Option<usize>,
}

pub fn tree(ui: &mut egui::Ui, project: &mut Project) -> OperationTreeActions {
    let mut actions = OperationTreeActions::default();
    ui.separator();
    ui.vertical(|ui| {
        // Se mantiene el ID anterior para conservar el estado abierto/cerrado.
        let root_id = ui.make_persistent_id("       LaguCAM");
        let mut root_clicked = false;
        let mut root_header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            root_id,
            true,
        )
        .show_header(ui, |ui| {
            root_clicked = tree_header_label(ui, TreeIcon::Project, "LaguCAM");
        });
        if root_clicked {
            root_header.toggle();
        }
        root_header.body(|ui| {
            let name = project
                .model
                .as_ref()
                .map_or("(sin STL)", |m| m.name.as_str());
            if icon_selectable_label(
                ui,
                project.selection == Selection::Model,
                &format!("Modelo STL · {name}"),
                TreeIcon::Model,
            )
            .clicked()
            {
                project.selection = Selection::Model;
            }
            if icon_selectable_label(
                ui,
                project.selection == Selection::Stock,
                "Bruto",
                TreeIcon::Stock,
            )
            .clicked()
            {
                project.selection = Selection::Stock;
            }
            if icon_selectable_label(
                ui,
                project.selection == Selection::Origin,
                "Origen CNC",
                TreeIcon::Origin,
            )
            .clicked()
            {
                project.selection = Selection::Origin;
            }
            ui.vertical(|ui| {
                let group_id = ui.make_persistent_id("       Operaciones");
                let mut group_clicked = false;
                let mut group_header =
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        group_id,
                        false,
                    )
                    .show_header(ui, |ui| {
                        group_clicked = tree_header_label(ui, TreeIcon::Operations, "Operaciones");
                    });
                if group_clicked {
                    group_header.toggle();
                }
                group_header.body(|ui| {
                    if project.operations.items.is_empty() {
                        ui.label("(vacía)");
                    }
                    for (i, op) in project.operations.items.iter().enumerate() {
                        let response = icon_selectable_label(
                            ui,
                            project.selection == Selection::Operation(i),
                            &op.name,
                            TreeIcon::Operation,
                        );
                        if response.clicked() {
                            project.selection = Selection::Operation(i);
                        }
                        if response.double_clicked() {
                            actions.edit = Some(i);
                        }
                    }
                    // La accion de crear siempre queda debajo de las operaciones existentes.
                    let count = project.operations.items.len();
                    if ui
                        .add_enabled(
                            project.model.is_some() && count < 32,
                            egui::Button::new("+ Nueva operación"),
                        )
                        .double_clicked()
                    {
                        if let Ok(index) = project.operations.add_new() {
                            project.selection = Selection::Operation(index);
                        }
                    }
                });
            });
        });
    });
    actions
}

pub fn properties(ui: &mut egui::Ui, project: &mut Project, error: &mut String) -> bool {
    let mut open_model = false;
    match project.selection {
        Selection::Model => {
            if let Some(model) = &project.model {
                ui.label(format!("Archivo: {}", model.name));
                ui.label(format!("Triángulos: {}", model.triangles));
                ui.label(format!("Tamaño STL: {} bytes", model.size_bytes));
                for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.label(format!(
                        "{axis}: {:.6} .. {:.6}",
                        model.bounds.min[i], model.bounds.max[i]
                    ));
                }
                ui.horizontal(|ui| {
                    ui.label("Color del modelo STL");
                    ui.color_edit_button_srgb(&mut project.model_color);
                });
                ui.small("El color conserva iluminación y relieve; el STL sigue opaco.");
                ui.small("Coordenadas originales f64. STL sin unidades definidas.");
            } else {
                ui.label("Abre un modelo para iniciar el proyecto.");
                if ui.button("Abrir modelo…").clicked() {
                    open_model = true;
                }
            }
        }
        Selection::Stock => {
            if let Some(stock) = &mut project.stock {
                ui.checkbox(&mut stock.visible, "Mostrar bruto");
                ui.label("Representación visual");
                egui::ComboBox::from_label("Modo")
                    .selected_text(project.stock_display.mode.label())
                    .show_ui(ui, |ui| {
                        for mode in StockMode::ALL {
                            ui.selectable_value(
                                &mut project.stock_display.mode,
                                mode,
                                mode.label(),
                            );
                        }
                    });
                egui::ComboBox::from_label("Color")
                    .selected_text(project.stock_display.color.label())
                    .show_ui(ui, |ui| {
                        for color in StockColor::ALL {
                            ui.selectable_value(
                                &mut project.stock_display.color,
                                color,
                                color.label(),
                            );
                        }
                    });
                ui.add_enabled(
                    project.stock_display.mode.has_faces(),
                    egui::Slider::new(&mut project.stock_display.opacity_percent, 0..=100)
                        .text("Opacidad (%)"),
                );
                let mut candidate = stock.clone();
                ui.label("Posición de esquina mínima (modelo)");
                let pos_changed = vector(ui, &mut candidate.min);
                ui.label("Dimensiones del bruto");
                let size_changed = vector(ui, &mut candidate.size);
                if pos_changed || size_changed {
                    match candidate.validate() {
                        Ok(()) => {
                            *stock = candidate;
                            error.clear();
                        }
                        Err(message) => *error = message.into(),
                    }
                }
                if let Some(model) = &project.model {
                    ui.separator();
                    ui.label("Material extra respecto a cada cara del STL");
                    ui.small("Orden: X-, X+, Y-, Y+, Z-, Z+. Unidades originales del STL.");
                    match stock.extras_from_bounds(&model.bounds) {
                        Ok(mut extras) => {
                            if extras.iter().any(|extra| *extra < 0.0) {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    "El bruto manual no contiene completamente el STL. Ajusta sus límites antes de usar extras.",
                                );
                            }
                            let mut changed = false;
                            for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{label}-"));
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut extras[2 * axis])
                                                .speed(0.1)
                                                .max_decimals(6)
                                                .range(0.0..=f64::MAX),
                                        )
                                        .changed();
                                    ui.label(format!("{label}+"));
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut extras[2 * axis + 1])
                                                .speed(0.1)
                                                .max_decimals(6)
                                                .range(0.0..=f64::MAX),
                                        )
                                        .changed();
                                });
                            }
                            if changed {
                                match crate::project::stock::Stock::from_extras(
                                    &model.bounds,
                                    extras,
                                    stock.visible,
                                ) {
                                    Ok(updated) => {
                                        *stock = updated;
                                        error.clear();
                                    }
                                    Err(message) => *error = message.into(),
                                }
                            }
                        }
                        Err(message) => {
                            ui.colored_label(egui::Color32::RED, message);
                        }
                    }
                    if ui.button("Ajustar a límites del STL").clicked() {
                        *stock = crate::project::stock::Stock::from_bounds(&model.bounds);
                        error.clear();
                    }
                }
                ui.small("El bruto conserva coordenadas f64; no altera el STL original.");
            } else {
                ui.label("Abre un STL primero.");
            }
        }
        Selection::Origin => {
            if let (Some(stock), Some(model)) = (&project.stock, &project.model) {
                let mut candidate = project.origin.clone();
                let mut changed = ui
                    .checkbox(&mut candidate.visible, "Mostrar ejes CNC")
                    .changed();
                ui.label("Referencia del cero CNC");
                let old_reference = candidate.reference;
                egui::ComboBox::from_label("Referencia")
                    .selected_text(candidate.reference.label())
                    .show_ui(ui, |ui| {
                        for reference in OriginReference::ALL {
                            ui.selectable_value(
                                &mut candidate.reference,
                                reference,
                                reference.label(),
                            );
                        }
                    });
                changed |= candidate.reference != old_reference;
                let old_point = candidate.point;
                egui::ComboBox::from_label("Punto")
                    .selected_text(candidate.point.label())
                    .show_ui(ui, |ui| {
                        for point in OriginPoint::ALL {
                            ui.selectable_value(&mut candidate.point, point, point.label());
                        }
                    });
                changed |= candidate.point != old_point;
                ui.small("También puedes pulsar los puntos rojos del visor. Aparecen solo al configurar el origen.");
                ui.separator();
                ui.label("Offset XYZ (ejes originales del modelo)");
                changed |= vector(ui, &mut candidate.offset);
                if ui.button("Restablecer offset a cero").clicked() {
                    candidate.offset = [0.0; 3];
                    changed = true;
                }
                ui.separator();
                ui.label("Rotación del sistema CNC (grados)");
                changed |= vector(ui, &mut candidate.rotation_degrees);
                ui.small("Convención: R = Rz · Ry · Rx, mano derecha.");
                if ui.button("Restablecer rotación a 0°").clicked() {
                    candidate.rotation_degrees = [0.0; 3];
                    changed = true;
                }
                if changed {
                    match (
                        candidate.model_position(stock, &model.bounds),
                        candidate.rotation_basis(),
                    ) {
                        (Ok(_), Ok(_)) => {
                            project.origin = candidate;
                            error.clear();
                        }
                        (Err(message), _) | (_, Err(message)) => *error = message.into(),
                    }
                }
                if let Ok(point) = project.origin.model_position(stock, &model.bounds) {
                    ui.label(format!(
                        "Origen en modelo: X {:.6}, Y {:.6}, Z {:.6}",
                        point[0], point[1], point[2]
                    ));
                }
                ui.small("Seleccionar un punto rojo restablece el offset a cero. La rotación solo orienta los ejes CNC.");
            } else {
                ui.label("Define primero el modelo y el bruto.");
            }
        }

        Selection::Operations => {
            ui.label(format!("Operaciones: {}", project.operations.items.len()));
        }
        Selection::Operation(i) => {
            if let Some(op) = project.operations.items.get(i) {
                ui.heading(&op.name);
                match &op.kind {
                    OperationKind::Placeholder => {
                        ui.label("Sin estrategia. Doble clic para seleccionar una.");
                    }
                    OperationKind::ParallelTest(p) => {
                        ui.label("Estrategia: Acabado paralelo — TEST");
                        ui.label(format!("Fresa esférica: Ø {:.3}", p.diameter));
                        ui.label(format!("Paso lateral: {:.3}", p.stepover));
                        ui.label(format!("Resolución: {:.3}", p.longitudinal));
                        ui.label(format!("Dirección: {}", if p.along_x { "X" } else { "Y" }));
                        match &op.status {
                            OperationStatus::NotCalculated => {
                                ui.label("Estado: pendiente de calcular")
                            }
                            OperationStatus::Ready => ui.label("Estado: calculada (vista previa)"),
                            OperationStatus::Error => ui.label("Estado: error de cálculo"),
                        };
                        ui.separator();
                        ui.small("Doble clic en la operación para editar y generar.");
                        ui.small("Parámetros guardados; trayectoria 3D no persistente.");
                    }
                };
            }
        }
    }
    if !error.is_empty() {
        ui.colored_label(egui::Color32::RED, error.as_str());
    }
    open_model
}

fn vector(ui: &mut egui::Ui, values: &mut [f64; 3]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
            ui.label(*label);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut values[i])
                        .speed(0.1)
                        .max_decimals(6),
                )
                .changed();
        }
    });
    changed
}

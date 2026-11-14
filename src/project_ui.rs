//! UI only: editable properties commit atomically after data-level validation.
use crate::project::{
    Project,
    display::{StockColor, StockMode},
    origin::{OriginPoint, OriginReference},
    selection::Selection,
};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, project: &mut Project, error: &mut String) {
    ui.heading("Proyecto");
    ui.separator();
    egui::CollapsingHeader::new("LaguCAM")
        .default_open(true)
        .show(ui, |ui| {
            let name = project
                .model
                .as_ref()
                .map_or("(sin STL)", |m| m.name.as_str());
            if ui
                .selectable_label(
                    project.selection == Selection::Model,
                    format!("Modelo STL · {name}"),
                )
                .clicked()
            {
                project.selection = Selection::Model;
            }
            if ui
                .selectable_label(project.selection == Selection::Stock, "Bruto")
                .clicked()
            {
                project.selection = Selection::Stock;
            }
            if ui
                .selectable_label(project.selection == Selection::Origin, "Origen CNC")
                .clicked()
            {
                project.selection = Selection::Origin;
            }
            if ui
                .selectable_label(project.selection == Selection::Tools, "Herramientas")
                .clicked()
            {
                project.selection = Selection::Tools;
            }
            ui.collapsing("Operaciones", |ui| {
                if ui
                    .selectable_label(
                        project.selection == Selection::Operations,
                        "Lista de operaciones",
                    )
                    .clicked()
                {
                    project.selection = Selection::Operations;
                }
                if project.operations.items.is_empty() {
                    ui.label("(vacía)");
                }
                for (i, op) in project.operations.items.iter().enumerate() {
                    if ui
                        .selectable_label(project.selection == Selection::Operation(i), &op.name)
                        .clicked()
                    {
                        project.selection = Selection::Operation(i);
                    }
                }
            });
        });
    ui.separator();
    ui.heading("Propiedades");
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
                ui.label("Abre un STL para iniciar el proyecto.");
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
        Selection::Tools => {
            ui.label("Herramientas: pendiente de desarrollo.");
        }
        Selection::Operations => {
            ui.label(format!("Operaciones: {}", project.operations.items.len()));
        }
        Selection::Operation(i) => {
            if let Some(op) = project.operations.items.get(i) {
                ui.label(&op.name);
            }
        }
    }
    if !error.is_empty() {
        ui.colored_label(egui::Color32::RED, error.as_str());
    }
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

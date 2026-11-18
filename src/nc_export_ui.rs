//! P2-B3: previsualizacion textual y exportacion del NC generado.
//!
//! Fuente: resultado CAM validado -> WorkFrame -> nc_post -> fichero NC.
//! El NC no se simula aqui: el interprete independiente llegara en P2-C.
//! El fichero se regenera justo al guardar, sin exportar una vista previa obsoleta.

use crate::tools::cam_adapter::ToolSignature;
use crate::{
    post_ui::PostUi,
    project::{Project, cam_coordinates::WorkFrame, operations::OperationStatus},
    test_parallel::TestParallelPanel,
};
use cam_libre_visor::{
    cam_contract::CuttingConditions,
    nc_post::{GeneratedNc, PostInput, PostMove, PostProcessorProfile, generate_program},
    toolpath_links::PlannedToolpath,
};
use eframe::egui;

#[derive(Clone)]
struct NcPreview {
    operation_index: usize,
    operation_name: String,
    profile_id: String,
    generated: GeneratedNc,
    tool_signature: ToolSignature,
}

#[derive(Default)]
pub struct NcExportUi {
    pub open: bool,
    selected_operation: Option<usize>,
    selected_profile: String,
    preview: Option<NcPreview>,
    acknowledge_unverified: bool,
    message: String,
    pending_playback: Option<(String, [f64; 3], ToolSignature)>,
}

/// Contrato puro: el post solo recibe coordenadas de trabajo en mm.
/// F/S NO reciben valores por defecto: la operacion debe configurarlos.
fn build_nc(
    frame: &WorkFrame,
    linked: &PlannedToolpath,
    conditions: CuttingConditions,
    profile: &PostProcessorProfile,
) -> Result<GeneratedNc, String> {
    let configured = conditions
        .require_configured()
        .map_err(|error| format!("Configura F y S en la estrategia antes de exportar: {error}"))?;
    let work = frame.convert(linked).map_err(|error| {
        format!("No se pueden convertir los movimientos al origen CNC: {error}")
    })?;
    let movements: Vec<PostMove> = work
        .moves
        .iter()
        .map(|movement| PostMove {
            kind: movement.kind,
            from_work_mm: movement.from_work_mm,
            to_work_mm: movement.to_work_mm,
            pass_index: movement.pass_index,
        })
        .collect();
    generate_program(
        profile,
        PostInput {
            moves_work_mm: &movements,
            conditions: configured,
        },
    )
    .map_err(|error| format!("No se puede generar el programa NC: {error}"))
}

/// Nombre sugerido. No acepta rutas ni separadores procedentes del nombre de la operacion.
fn suggested_filename(name: &str, extension: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let stem = if stem.trim_matches('_').is_empty() {
        "lagucam_operacion"
    } else {
        &stem
    };
    format!("{stem}.{extension}")
}

fn preview_excerpt(text: &str) -> String {
    const HEAD: usize = 65;
    const TAIL: usize = 22;
    let count = text.lines().count();
    if count <= HEAD + TAIL {
        return text.to_owned();
    }
    let head = text.lines().take(HEAD).collect::<Vec<_>>().join("\n");
    let mut tail = text.lines().rev().take(TAIL).collect::<Vec<_>>();
    tail.reverse();
    format!(
        "{head}\n\n... [se omiten {} lineas de la vista; el fichero se guarda completo] ...\n\n{}\n",
        count - HEAD - TAIL,
        tail.join("\n")
    )
}

impl NcExportUi {
    /// Devuelve una unica peticion; se envia el texto NC completo, no la trayectoria CAM.
    pub fn take_playback_request(&mut self) -> Option<(String, [f64; 3], ToolSignature)> {
        self.pending_playback.take()
    }

    fn generate_current(
        &self,
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
        index: usize,
    ) -> Result<NcPreview, String> {
        let operation = project
            .operations
            .items
            .get(index)
            .ok_or("La operacion elegida ya no existe")?;
        if operation.status != OperationStatus::Ready {
            return Err(
                "La operacion no dispone de una trayectoria CAM lista; vuelve a calcularla".into(),
            );
        }
        let (linked, conditions) = panel
            .nc_result(index)
            .ok_or("El resultado CAM ha sido invalidado o no contiene cortes")?;
        let tool_signature = panel
            .nc_tool_signature(index)
            .ok_or("La operacion ya no conserva procedencia de herramienta CAM")?;
        if operation.tool_id.as_deref() != Some(tool_signature.tool_id.as_str()) {
            return Err(
                "La herramienta asignada no coincide con el calculo CAM; vuelve a calcular".into(),
            );
        }
        let model = project.model.as_ref().ok_or("No hay modelo cargado")?;
        let stock = project.stock.as_ref().ok_or("No hay bruto definido")?;
        let frame = WorkFrame::new(&project.origin, stock, &model.bounds).map_err(|error| {
            format!("Origen CNC no admitido por este postprocesador de tres ejes: {error}")
        })?;
        let generated = build_nc(&frame, linked, conditions, profile)?;
        Ok(NcPreview {
            operation_index: index,
            operation_name: operation.name.clone(),
            profile_id: profile.id.clone(),
            generated,
            tool_signature,
        })
    }

    fn save_current(
        &mut self,
        project: &Project,
        panel: &TestParallelPanel,
        profile: &PostProcessorProfile,
        index: usize,
    ) -> Result<(), String> {
        if !self.acknowledge_unverified {
            return Err("Confirma primero las limitaciones del NC no simulado".into());
        }
        let previous = self
            .preview
            .as_ref()
            .ok_or("Genera primero la vista previa")?;
        if previous.operation_index != index || previous.profile_id != profile.id {
            return Err("La operacion o el perfil han cambiado; regenera la vista previa".into());
        }
        let current = self.generate_current(project, panel, profile, index)?;
        if current.operation_name != previous.operation_name
            || current.tool_signature != previous.tool_signature
            || current.generated != previous.generated
        {
            self.preview = None;
            self.acknowledge_unverified = false;
            return Err("El NC ha cambiado desde la vista previa (origen, CAM o perfil). Regenera la vista previa antes de exportar".into());
        }
        let suggestion = suggested_filename(&current.operation_name, &profile.extension);
        let path = rfd::FileDialog::new()
            .add_filter("Programa CNC", &[profile.extension.as_str()])
            .set_file_name(&suggestion)
            .save_file();
        let Some(path) = path else {
            return Ok(());
        };
        if !path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(&profile.extension))
        {
            return Err(format!(
                "Selecciona un archivo con extension .{}",
                profile.extension
            ));
        }
        // No se toca el STL ni .lagucam. Se escribe un temporal completo antes
        // de reemplazar el destino seleccionado (si el usuario confirma).
        crate::portable::atomic_write(&path, current.generated.text.as_bytes())?;
        self.message = format!(
            "NC exportado: {}. SIN simulacion NC ni verificacion de maquina.",
            path.display()
        );
        Ok(())
    }

    /// Ventana flotante de compatibilidad; comparte exactamente la misma UI.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        panel: &TestParallelPanel,
        post_ui: &PostUi,
    ) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Postprocesar programa NC")
            .open(&mut open)
            .default_size([870.0, 680.0])
            .min_width(570.0)
            .resizable(true)
            .show(ctx, |ui| self.show_embedded(ui, project, panel, post_ui));
        self.open = open;
    }

    /// Contenido compartido por ventana y panel de mecanizado.
    pub fn show_embedded(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        panel: &TestParallelPanel,
        post_ui: &PostUi,
    ) {
        let available: Vec<(usize, String)> = project
            .operations
            .items
            .iter()
            .enumerate()
            .filter(|(i, op)| op.status == OperationStatus::Ready && panel.nc_result(*i).is_some())
            .map(|(i, op)| (i, op.name.clone()))
            .collect();
        let profiles = post_ui.profiles_for_export();
        if !available
            .iter()
            .any(|(i, _)| Some(*i) == self.selected_operation)
        {
            self.selected_operation = available.first().map(|(i, _)| *i);
            self.preview = None;
            self.acknowledge_unverified = false;
        }
        if !profiles.iter().any(|p| p.id == self.selected_profile) {
            self.selected_profile = profiles.first().map_or_else(String::new, |p| p.id.clone());
            self.preview = None;
            self.acknowledge_unverified = false;
        }

        let mut operation = self.selected_operation;
        let mut profile_id = self.selected_profile.clone();
        let mut acknowledge = self.acknowledge_unverified;
        let mut preview_requested = false;
        let mut export_requested = false;
        let mut playback_requested = false;

        ui.heading("Vista previa y exportacion G-code");
        ui.small("Solo operaciones CAM calculadas, con cortes y validadas estructuralmente.");
        if available.is_empty() {
            ui.colored_label(egui::Color32::YELLOW,
                        "No hay una operacion CAM lista. Calcula primero un acabado paralelo con contactos y cortes.");
        }
        ui.horizontal(|ui| {
            ui.label("Operacion");
            let selected_name = available
                .iter()
                .find(|(i, _)| Some(*i) == operation)
                .map_or("Sin operacion", |(_, name)| name.as_str());
            egui::ComboBox::from_id_salt("p2b3_operation")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for (i, name) in &available {
                        ui.selectable_value(&mut operation, Some(*i), name.as_str());
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Postprocesador");
            let selected_name = profiles
                .iter()
                .find(|p| p.id == profile_id)
                .map_or("Sin perfil", |p| p.name.as_str());
            egui::ComboBox::from_id_salt("p2b3_profile")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for p in &profiles {
                        ui.selectable_value(&mut profile_id, p.id.clone(), p.name.as_str());
                    }
                });
        });
        ui.small("Solo se utilizan perfiles guardados en la biblioteca de postprocesadores.");
        if profiles.is_empty() {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "No hay perfiles disponibles. Revisa la biblioteca de postprocesadores.",
            );
        }
        if let Some(profile) = profiles.iter().find(|p| p.id == profile_id) {
            ui.label(format!(
                "Controlador: {} · NC: {} · extension: .{} · XYZ: {} decimales",
                profile.controller,
                profile.units.code(),
                profile.extension,
                profile.xyz_decimals
            ));
            if !profile.start_commands.is_empty() || !profile.end_commands.is_empty() {
                ui.colored_label(egui::Color32::YELLOW,
                            "Este perfil contiene Start/End personalizados: pueden introducir movimientos adicionales.");
            }
        }
        ui.separator();
        let selected_matches_preview = self
            .preview
            .as_ref()
            .is_some_and(|p| Some(p.operation_index) == operation && p.profile_id == profile_id);
        if ui
            .add_enabled(
                operation.is_some() && !profiles.is_empty(),
                egui::Button::new("Generar vista previa NC"),
            )
            .clicked()
        {
            preview_requested = true;
        }
        if selected_matches_preview && let Some(preview) = self.preview.as_ref() {
            let generated = &preview.generated;
            ui.small(format!(
                "Herramienta de origen CAM: {} (UUID + SHA-256 geometrico)",
                preview.tool_signature.tool_id
            ));
            ui.label(format!(
                "{} movimientos CAM · {} lineas NC · {} bytes",
                generated.cam_move_count,
                generated.text.lines().count(),
                generated.text.len()
            ));
            let xyz = generated.required_first_move_position_work_mm;
            ui.colored_label(egui::Color32::YELLOW, format!(
                        "PRECONDICION: antes del primer movimiento CAM, herramienta en coordenadas de trabajo X{:.6} Y{:.6} Z{:.6} mm.",
                        xyz[0], xyz[1], xyz[2]
                    ));
            ui.colored_label(egui::Color32::YELLOW,
                        "NO interpretado ni simulado: Start/End, origen fisico, colisiones y estado inicial no verificados.");
            ui.separator();
            ui.label("Vista textual: primeras 65 y ultimas 22 lineas si el NC es grande.");
            let excerpt = preview_excerpt(&generated.text);
            egui::ScrollArea::vertical()
                .max_height(270.0)
                .show(ui, |ui| {
                    ui.monospace(excerpt);
                });
            ui.separator();
            if ui.button("Reproducir G-code interpretado...").clicked() {
                playback_requested = true;
            }
            ui.checkbox(&mut acknowledge,
                        "Reconozco que este NC aun no tiene simulacion final y requiere verificar origen, posicion inicial, herramienta, Start/End y maquina.");
            if ui
                .add_enabled(acknowledge, egui::Button::new("Exportar archivo NC..."))
                .clicked()
            {
                export_requested = true;
            }
        }
        if !self.message.is_empty() {
            ui.separator();
            ui.label(&self.message);
        }
        if operation != self.selected_operation || profile_id != self.selected_profile {
            self.preview = None;
            self.acknowledge_unverified = false;
            self.message.clear();
        } else {
            self.acknowledge_unverified = acknowledge;
        }
        self.selected_operation = operation;
        self.selected_profile = profile_id;

        if preview_requested {
            let result = self
                .selected_operation
                .ok_or_else(|| "Selecciona una operacion CAM calculada".to_owned())
                .and_then(|i| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "Selecciona un postprocesador guardado".to_owned())?;
                    self.generate_current(project, panel, profile, i)
                });
            match result {
                Ok(preview) => {
                    self.preview = Some(preview);
                    self.acknowledge_unverified = false;
                    self.message = "NC generado en memoria. Revisa la precondicion y el texto antes de exportar.".into();
                }
                Err(error) => {
                    self.preview = None;
                    self.acknowledge_unverified = false;
                    self.message = format!("ERROR: {error}");
                }
            }
        }
        if playback_requested {
            // Regenerar antes de enviar, igual que al exportar. Un preview obsoleto se rechaza.
            let result = self
                .selected_operation
                .ok_or_else(|| "No hay operacion seleccionada".to_owned())
                .and_then(|index| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "No hay perfil guardado seleccionado".to_owned())?;
                    let current = self.generate_current(project, panel, profile, index)?;
                    let previous = self
                        .preview
                        .as_ref()
                        .ok_or_else(|| "Genera primero la vista previa NC".to_owned())?;
                    if current.operation_index != previous.operation_index
                        || current.operation_name != previous.operation_name
                        || current.profile_id != previous.profile_id
                        || current.tool_signature != previous.tool_signature
                        || current.generated != previous.generated
                    {
                        return Err(
                            "La vista NC ha quedado obsoleta; regenerala antes de reproducir"
                                .to_owned(),
                        );
                    }
                    Ok((
                        current.generated.text.clone(),
                        current.generated.required_first_move_position_work_mm,
                        current.tool_signature.clone(),
                    ))
                });
            match result {
                Ok(request) => {
                    self.pending_playback = Some(request);
                    self.message =
                        "NC enviado al reproductor; confirma la posicion inicial ANTES de Start."
                            .into();
                }
                Err(error) => {
                    self.pending_playback = None;
                    self.preview = None;
                    self.message = format!("ERROR: {error}");
                }
            }
        }
        if export_requested {
            let result = self
                .selected_operation
                .ok_or_else(|| "No hay operacion seleccionada".to_owned())
                .and_then(|index| {
                    let profile = profiles
                        .iter()
                        .find(|p| p.id == self.selected_profile)
                        .ok_or_else(|| "El perfil ha desaparecido".to_owned())?;
                    self.save_current(project, panel, profile, index)
                });
            if let Err(error) = result {
                self.message = format!("ERROR: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        project::{origin::WorkOrigin, stock::Stock},
        stl::Bounds,
    };
    use cam_libre_visor::{
        nc_post::PostUnits,
        toolpath_links::{PlannedMove, PlannedMoveKind},
    };

    fn frame() -> WorkFrame {
        let bounds = Bounds {
            min: [0.0, 0.0, 0.0],
            max: [30.0, 30.0, 30.0],
        };
        let stock = Stock {
            min: [0.0, 0.0, 0.0],
            size: [30.0, 30.0, 30.0],
            visible: true,
        };
        WorkFrame::new(&WorkOrigin::default(), &stock, &bounds).unwrap()
    }
    fn path() -> PlannedToolpath {
        PlannedToolpath {
            moves: vec![
                PlannedMove {
                    kind: PlannedMoveKind::RapidTraverse,
                    from: [0.0, 0.0, 25.4],
                    to: [25.4, 0.0, 25.4],
                    pass_index: 0,
                },
                PlannedMove {
                    kind: PlannedMoveKind::Approach,
                    from: [25.4, 0.0, 25.4],
                    to: [25.4, 0.0, 12.7],
                    pass_index: 0,
                },
            ],
            ..Default::default()
        }
    }
    fn conditions() -> CuttingConditions {
        CuttingConditions {
            feed_mm_min: Some(254.0),
            spindle_rpm: Some(14000.0),
        }
    }

    #[test]
    fn model_to_work_happens_before_post_and_preserves_initial_precondition() {
        let frame = frame();
        let expected = frame.model_to_work(path().moves[0].from).unwrap();
        let nc = build_nc(
            &frame,
            &path(),
            conditions(),
            &PostProcessorProfile::grbl_3axis_mm(),
        )
        .unwrap();
        assert_eq!(nc.required_first_move_position_work_mm, expected);
        assert!(nc.text.contains("G90\n"));
        assert!(nc.text.contains("G0 "));
        assert!(nc.text.contains("G1 "));
        assert!(nc.requires_nc_interpretation);
    }

    #[test]
    fn independent_output_units_convert_feed_but_not_spindle() {
        let mut profile = PostProcessorProfile::grbl_3axis_mm();
        profile.units = PostUnits::Inches;
        let nc = build_nc(&frame(), &path(), conditions(), &profile).unwrap();
        assert!(nc.text.contains("G20\n"));
        assert!(nc.text.contains("F10.000"));
        assert!(nc.text.contains("S14000\n"));
    }

    #[test]
    fn missing_conditions_cannot_be_silently_invented() {
        let error = build_nc(
            &frame(),
            &path(),
            CuttingConditions::default(),
            &PostProcessorProfile::grbl_3axis_mm(),
        )
        .unwrap_err();
        assert!(error.contains("F y S"));
    }

    #[test]
    fn filename_cannot_become_a_path_and_excerpt_contains_both_ends() {
        assert_eq!(
            suggested_filename("../../pieza * a", "nc"),
            "______pieza___a.nc"
        );
        let body = (0..100)
            .map(|i| format!("N{i} G1 X{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt = preview_excerpt(&body);
        assert!(excerpt.contains("N0 G1 X0"));
        assert!(excerpt.contains("N99 G1 X99"));
        assert!(excerpt.contains("se omiten"));
    }
}

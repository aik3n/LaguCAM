//! Superposición puramente visual de perfiles CSV generados por primera_muestra_superficie.
//! No calcula trayectorias, no modifica proyectos y no interpreta los puntos como G-code.
use crate::project::model::ModelReference;
use eframe::egui;
use glam::Vec3;
use std::{fs, path::Path};

pub const PASSES: usize = 5;
pub const SAMPLES_PER_PASS: usize = 81;
const MAX_CSV_BYTES: u64 = 1024 * 1024;
const HEADER: &str = "corte,punto,x_stl,y_stl,z_punta_esferica,hay_contacto";

/// Coordenadas en el espacio ORIGINAL del STL, no en el espacio CNC ni en la GPU.
pub struct Profiles {
    passes: [Vec<Option<[f64; 3]>>; PASSES],
    source: &'static str,
}

impl Profiles {
    /// Perfiles creados por el motor geométrico con el STL abierto.
    pub fn from_computed_passes(passes: [Vec<Option<[f64; 3]>>; PASSES]) -> Result<Self, String> {
        for row in &passes {
            if row.len() != SAMPLES_PER_PASS
                || !row
                    .iter()
                    .flatten()
                    .all(|p| p.iter().all(|v| v.is_finite()))
            {
                return Err("Resultado de perfiles incompleto o con coordenadas inválidas".into());
            }
        }
        Ok(Self {
            passes,
            source: "CALCULADA",
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| format!("No se puede abrir el CSV: {e}"))?;
        if !metadata.is_file() || metadata.len() > MAX_CSV_BYTES {
            return Err("El CSV no es un archivo regular o supera 1 MiB".into());
        }
        let bytes = fs::read(path).map_err(|e| format!("Error leyendo CSV: {e}"))?;
        let input = std::str::from_utf8(&bytes).map_err(|_| "CSV no codificado en UTF-8")?;
        Self::parse(input)
    }

    fn parse(input: &str) -> Result<Self, String> {
        let mut lines = input.lines();
        if lines.next().map(str::trim_end) != Some(HEADER) {
            return Err("Cabecera CSV incorrecta: selecciona el CSV de la primera muestra".into());
        }
        let mut passes: [Vec<Option<[f64; 3]>>; PASSES] =
            std::array::from_fn(|_| Vec::with_capacity(SAMPLES_PER_PASS));
        for pass in 0..PASSES {
            for point in 0..SAMPLES_PER_PASS {
                let line = lines.next().ok_or("CSV incompleto: faltan puntos")?;
                let fields: Vec<&str> = line.trim_end_matches('\r').split(',').collect();
                if fields.len() != 6
                    || fields[0].parse::<usize>().ok() != Some(pass + 1)
                    || fields[1].parse::<usize>().ok() != Some(point + 1)
                {
                    return Err(format!(
                        "Orden o columnas inválidas en corte {}, punto {}",
                        pass + 1,
                        point + 1
                    ));
                }
                let x = fields[2]
                    .parse::<f64>()
                    .map_err(|_| "Coordenada X inválida")?;
                let y = fields[3]
                    .parse::<f64>()
                    .map_err(|_| "Coordenada Y inválida")?;
                if !x.is_finite() || !y.is_finite() {
                    return Err("Coordenadas XY no finitas".into());
                }
                let sample = match fields[5] {
                    "si" => {
                        let z = fields[4].parse::<f64>().map_err(|_| "Altura Z inválida")?;
                        if !z.is_finite() {
                            return Err("Altura Z no finita".into());
                        }
                        Some([x, y, z])
                    }
                    "no" if fields[4].is_empty() => None,
                    _ => return Err("Columna de contacto incoherente".into()),
                };
                passes[pass].push(sample);
            }
        }
        if lines.any(|line| !line.trim().is_empty()) {
            return Err("El CSV contiene puntos o datos adicionales".into());
        }
        Ok(Self {
            passes,
            source: "CSV",
        })
    }

    /// Dibuja por delante de la escena; no hace prueba de profundidad (visualización de rayos X).
    /// El Z representa la punta de la fresa esférica, NO necesariamente la superficie del STL.
    pub fn paint(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        model: &ModelReference,
        yaw: f32,
        pitch: f32,
        zoom: f32,
        pan: Vec3,
        projection: crate::navigation::Projection,
    ) {
        let aspect = rect.width() / rect.height().max(1.0);
        let mvp =
            crate::navigation::view_projection_mode(yaw, pitch, zoom, aspect, pan, projection);
        let painter = ui.painter().with_clip_rect(rect);
        let colors = [
            egui::Color32::from_rgb(45, 225, 230),
            egui::Color32::from_rgb(255, 205, 60),
            egui::Color32::from_rgb(248, 105, 165),
            egui::Color32::from_rgb(148, 245, 111),
            egui::Color32::from_rgb(184, 137, 252),
        ];
        for (index, points) in self.passes.iter().enumerate() {
            let stroke = egui::Stroke::new(2.0, colors[index]);
            let mut prev: Option<egui::Pos2> = None;
            for sample in points {
                let screen = sample
                    .and_then(|xyz| model.visual_point(xyz))
                    .and_then(|v| {
                        let clip = mvp * v.extend(1.0);
                        if !clip.is_finite() || clip.w <= 0.0 {
                            return None;
                        }
                        let ndc = clip.truncate() / clip.w;
                        if !ndc.is_finite()
                            || ndc.x.abs() > 1.0
                            || ndc.y.abs() > 1.0
                            || !(-1.0..=1.0).contains(&ndc.z)
                        {
                            return None;
                        }
                        Some(
                            rect.center()
                                + egui::vec2(
                                    ndc.x * rect.width() * 0.5,
                                    -ndc.y * rect.height() * 0.5,
                                ),
                        )
                    });
                if let (Some(a), Some(b)) = (prev, screen) {
                    painter.line_segment([a, b], stroke);
                }
                prev = screen;
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(12.0, 32.0),
            egui::Align2::LEFT_TOP,
            format!(
                "MUESTRA {} · 5 perfiles · Z punta esférica · sin ocultación",
                self.source
            ),
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_csv() -> String {
        let mut s = format!("{HEADER}\n");
        for pass in 1..=PASSES {
            for point in 1..=SAMPLES_PER_PASS {
                s.push_str(&format!("{pass},{point},{point}.0,{pass}.0,10.0,si\n"));
            }
        }
        s
    }

    #[test]
    fn computed_profiles_require_complete_finite_rows() {
        let mut passes: [Vec<Option<[f64; 3]>>; PASSES] =
            std::array::from_fn(|_| vec![Some([1.0, 2.0, 3.0]); SAMPLES_PER_PASS]);
        assert!(Profiles::from_computed_passes(passes.clone()).is_ok());
        passes[0].pop();
        assert!(Profiles::from_computed_passes(passes.clone()).is_err());
        passes[0].push(Some([0.0, f64::NAN, 1.0]));
        assert!(Profiles::from_computed_passes(passes).is_err());
    }

    #[test]
    fn reads_exactly_five_passes_with_81_points() {
        let profiles = Profiles::parse(&sample_csv()).unwrap();
        assert_eq!(profiles.passes.len(), 5);
        assert_eq!(profiles.passes[0].len(), 81);
        assert_eq!(profiles.passes[4][80], Some([81.0, 5.0, 10.0]));
    }

    #[test]
    fn missing_contact_keeps_a_gap() {
        let mut csv = sample_csv();
        csv = csv.replacen("1,2,2.0,1.0,10.0,si", "1,2,2.0,1.0,,no", 1);
        let profiles = Profiles::parse(&csv).unwrap();
        assert_eq!(profiles.passes[0][1], None);
        assert!(profiles.passes[0][2].is_some());
    }

    #[test]
    fn rejects_extra_point_and_nonfinite_geometry() {
        let csv = sample_csv();
        assert!(Profiles::parse(&(csv.clone() + "1,82,82,1,1,si\n")).is_err());
        assert!(Profiles::parse(&csv.replacen("1,1,1.0", "1,1,NaN", 1)).is_err());
    }
}

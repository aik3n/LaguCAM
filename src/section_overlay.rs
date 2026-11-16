//! Vista 3D de secciones Y=constante obtenidas del STL original.
//! Solo representacion. No hay herramienta, trayectoria CAM ni G-code.
use crate::{project::model::ModelReference, stl::Bounds};
use eframe::egui;
use glam::Vec3;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

const HEADER: &str = "seccion,y,x1,z1,x2,z2,tipo";
const FRACTIONS: [f64; 5] = [0.15, 0.325, 0.5, 0.675, 0.85];
const MAX_FILE_BYTES: u64 = 96 * 1024 * 1024;
const MAX_PIECES: usize = 350_000;
const MAX_DRAW_PIECES: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Segment,
    Point,
    CoplanarEdge,
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    a: [f64; 2], // X,Z STL original
    b: [f64; 2],
    kind: Kind,
}

pub struct Sections {
    per_plane: [Vec<Piece>; 5],
    planes_y: [f64; 5],
    total_pieces: usize,
}

impl Sections {
    pub fn load(path: &Path, model: &ModelReference) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| format!("CSV inaccesible: {e}"))?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err("CSV de secciones no regular o mayor de 96 MiB".into());
        }
        let f = File::open(path).map_err(|e| format!("No se pudo abrir el CSV: {e}"))?;
        Self::parse(BufReader::with_capacity(256 * 1024, f), model.bounds)
    }

    fn parse<R: BufRead>(reader: R, bounds: Bounds) -> Result<Self, String> {
        let span_y = bounds.max[1] - bounds.min[1];
        if !span_y.is_finite() || span_y <= 0.0 {
            return Err("El STL abierto no tiene extension positiva en Y".into());
        }
        let planes_y = FRACTIONS.map(|f| bounds.min[1] + span_y * f);
        let mut per_plane: [Vec<Piece>; 5] = std::array::from_fn(|_| Vec::new());
        let mut lines = reader.lines();
        if lines
            .next()
            .transpose()
            .map_err(|e| e.to_string())?
            .as_deref()
            .map(str::trim_end)
            != Some(HEADER)
        {
            return Err(
                "Este CSV no contiene secciones reales; elige LaguCAM_secciones_reales_*.csv"
                    .into(),
            );
        }
        let mut total_pieces = 0usize;
        for (row, line) in lines.enumerate() {
            let line = line.map_err(|e| format!("Error de lectura en linea {}: {e}", row + 2))?;
            if line.trim().is_empty() {
                continue;
            }
            if total_pieces >= MAX_PIECES {
                return Err("El CSV supera el limite de 350000 segmentos".into());
            }
            let f: Vec<_> = line.split(',').collect();
            if f.len() != 7 {
                return Err(format!("Columnas invalidas en linea {}", row + 2));
            }
            let index = f[0]
                .parse::<usize>()
                .map_err(|_| "Indice de seccion incorrecto")?;
            if !(1..=5).contains(&index) {
                return Err(format!(
                    "Numero de seccion fuera de 1..5 en linea {}",
                    row + 2
                ));
            }
            let i = index - 1;
            let parse_num = |s: &str| -> Result<f64, String> {
                let value = s
                    .parse::<f64>()
                    .map_err(|_| "Coordenada CSV no numerica".to_string())?;
                if !value.is_finite() {
                    return Err("Coordenada CSV no finita".into());
                }
                Ok(value)
            };
            let y = parse_num(f[1])?;
            // La salida original usa {:?}, que conserva los bits f64 al releer.
            // Este control detecta muchos casos de CSV procedentes de otro STL.
            if y.to_bits() != planes_y[i].to_bits() {
                return Err(format!(
                    "Seccion {} no corresponde a los limites Y del STL abierto",
                    index
                ));
            }
            let a = [parse_num(f[2])?, parse_num(f[3])?];
            let b = [parse_num(f[4])?, parse_num(f[5])?];
            if !inside_bounds(a, bounds) || !inside_bounds(b, bounds) {
                return Err(format!(
                    "Seccion {} tiene puntos fuera de los limites X/Z del STL",
                    index
                ));
            }
            let kind = match f[6] {
                "segmento" => Kind::Segment,
                "punto" => Kind::Point,
                "arista_coplanar" => Kind::CoplanarEdge,
                _ => return Err(format!("Tipo de seccion desconocido en linea {}", row + 2)),
            };
            if kind == Kind::Point && a != b {
                return Err("Punto aislado con extremos distintos".into());
            }
            per_plane[i].push(Piece { a, b, kind });
            total_pieces += 1;
        }
        if total_pieces == 0 {
            return Err("El CSV de secciones esta vacio".into());
        }
        Ok(Self {
            per_plane,
            planes_y,
            total_pieces,
        })
    }

    pub fn piece_count(&self) -> usize {
        self.total_pieces
    }

    /// Superposicion 2D proyectada sobre la camara. No utiliza profundidad;
    /// las secciones detras del relieve tambien son visibles (modo rayos X).
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
            egui::Color32::from_rgb(40, 224, 235),
            egui::Color32::from_rgb(255, 205, 50),
            egui::Color32::from_rgb(255, 110, 168),
            egui::Color32::from_rgb(150, 245, 95),
            egui::Color32::from_rgb(198, 149, 255),
        ];
        let stride = self.total_pieces.div_ceil(MAX_DRAW_PIECES).max(1);
        let screen = |xyz: [f64; 3]| -> Option<egui::Pos2> {
            let pos = model.visual_point(xyz)?;
            let clip = mvp * pos.extend(1.0);
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
                    + egui::vec2(ndc.x * rect.width() * 0.5, -ndc.y * rect.height() * 0.5),
            )
        };
        for (i, pieces) in self.per_plane.iter().enumerate() {
            let stroke = egui::Stroke::new(1.8, colors[i]);
            for (j, piece) in pieces.iter().enumerate() {
                if j % stride != 0 {
                    continue;
                }
                let y = self.planes_y[i];
                let a = screen([piece.a[0], y, piece.a[1]]);
                if piece.kind == Kind::Point {
                    if let Some(a) = a {
                        painter.circle_filled(a, 1.8, colors[i]);
                    }
                } else if let (Some(a), Some(b)) = (a, screen([piece.b[0], y, piece.b[1]])) {
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        let description = if stride == 1 {
            format!(
                "SECCIONES REALES · {} segmentos · sin herramienta · vista rayos X",
                self.total_pieces
            )
        } else {
            format!(
                "SECCIONES REALES · {} segmentos · dibujo reducido 1/{} · vista rayos X",
                self.total_pieces, stride
            )
        };
        painter.text(
            rect.left_top() + egui::vec2(12.0, 48.0),
            egui::Align2::LEFT_TOP,
            description,
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );
    }
}

fn inside_bounds(p: [f64; 2], bounds: Bounds) -> bool {
    [(0, p[0]), (2, p[1])].into_iter().all(|(axis, v)| {
        let lo = bounds.min[axis];
        let hi = bounds.max[axis];
        let tol = (hi - lo).abs().max(1.0) * 1e-9;
        v >= lo - tol && v <= hi + tol
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bounds() -> Bounds {
        Bounds {
            min: [0.0, 0.0, 0.0],
            max: [100.0, 100.0, 50.0],
        }
    }
    fn csv() -> String {
        let mut s = format!("{HEADER}\n");
        for (i, fraction) in FRACTIONS.iter().enumerate() {
            s.push_str(&format!(
                "{},{:?},10.0,20.0,10.0,40.0,segmento\n",
                i + 1,
                100.0 * fraction
            ));
        }
        s
    }
    #[test]
    fn lee_cinco_secciones_independientes_con_pared_vertical() {
        let sections = Sections::parse(Cursor::new(csv()), bounds()).unwrap();
        assert_eq!(sections.piece_count(), 5);
        assert_eq!(sections.per_plane[0][0].a, [10.0, 20.0]);
        assert_eq!(sections.per_plane[0][0].b, [10.0, 40.0]);
    }
    #[test]
    fn rechaza_csv_de_otro_modelo_y_numeros_invalidos() {
        let altered = csv().replacen("1,15.0,", "1,25.0,", 1);
        assert!(Sections::parse(Cursor::new(altered), bounds()).is_err());
        let altered = csv().replacen("10.0,20.0", "NaN,20.0", 1);
        assert!(Sections::parse(Cursor::new(altered), bounds()).is_err());
    }
    #[test]
    fn rechaza_csv_de_perfiles_de_fresa() {
        let input = "corte,punto,x_stl,y_stl,z_punta_esferica,hay_contacto\n";
        assert!(Sections::parse(Cursor::new(input), bounds()).is_err());
    }
}

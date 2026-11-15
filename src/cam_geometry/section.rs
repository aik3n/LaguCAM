//! Secciones del STL binario original, independientes de la herramienta.
//!
//! Una seccion Y=constante produce segmentos (X,Z) y puntos aislados.
//! No ordena ni une segmentos por proximidad: una malla puede tener varias
//! superficies para una misma X, paredes verticales y componentes separados.
//! Los triangulos coplanares aportan sus tres aristas (incluidas las internas).
//! Se decodifican los vertices f32 originales y se opera en f64; esta no es
//! una representacion exacta de una superficie anterior al STL.
use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

const STL_HEADER: u64 = 84;
const STL_RECORD: u64 = 50;
const CANCEL_PERIOD: u32 = 8_192;

#[derive(Debug)]
pub enum SectionError {
    Io(io::Error),
    InvalidStl(&'static str),
    InvalidVertex(u32),
    InvalidInput(&'static str),
    LimitExceeded(u64),
    ChangedDuringRead,
    Cancelled,
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Lectura o escritura: {e}"),
            Self::InvalidStl(m) => write!(f, "STL invalido: {m}"),
            Self::InvalidVertex(i) => write!(f, "Coordenada no finita en triangulo {i}"),
            Self::InvalidInput(m) => write!(f, "Parametro invalido: {m}"),
            Self::LimitExceeded(n) => write!(f, "Demasiados registros de seccion (limite {n})"),
            Self::ChangedDuringRead => write!(f, "El STL cambio durante la lectura"),
            Self::Cancelled => write!(f, "Lectura cancelada"),
        }
    }
}
impl std::error::Error for SectionError {}
impl From<io::Error> for SectionError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds3 {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    fn include(&mut self, triangle: &[[f64; 3]; 3]) {
        for v in triangle {
            for k in 0..3 {
                self.min[k] = self.min[k].min(v[k]);
                self.max[k] = self.max[k].max(v[k]);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Segment,
    Point,
    CoplanarEdge,
}

#[derive(Clone, Copy, Debug)]
pub struct SectionPiece {
    pub a: [f64; 2], // x,z en coordenadas originales del STL
    pub b: [f64; 2],
    pub kind: PieceKind,
}

#[derive(Debug)]
pub struct SectionSummary {
    pub bounds: Bounds3,
    pub triangles: u32,
    pub planes_y: Vec<f64>,
    pub records_per_plane: Vec<u64>,
    pub isolated_points: u64,
    pub coplanar_triangles: u64,
    pub total_records: u64,
}

pub trait SectionSink {
    fn start(&mut self, bounds: &Bounds3, planes_y: &[f64]) -> io::Result<()>;
    fn piece(&mut self, plane_index: usize, piece: SectionPiece) -> io::Result<()>;
    fn finish(&mut self, _summary: &SectionSummary) -> io::Result<()> {
        Ok(())
    }
}

fn protected_reader(path: &Path) -> io::Result<BufReader<File>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Impide nuevos manejadores de escritura mientras el STL esta abierto.
        // No garantiza una instantanea frente a escritores preexistentes.
        options.share_mode(0x0000_0001); // FILE_SHARE_READ
    }
    Ok(BufReader::with_capacity(
        4 * 1024 * 1024,
        options.open(path)?,
    ))
}

fn check_cancel(cancel: Option<&Path>) -> Result<(), SectionError> {
    if cancel.is_some_and(Path::exists) {
        Err(SectionError::Cancelled)
    } else {
        Ok(())
    }
}

fn triangle_from_record(data: &[u8; 50], i: u32) -> Result<[[f64; 3]; 3], SectionError> {
    let mut tri = [[0f64; 3]; 3];
    for v in 0..3 {
        for axis in 0..3 {
            let pos = 12 + v * 12 + axis * 4;
            let value = f32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            if !value.is_finite() {
                return Err(SectionError::InvalidVertex(i));
            }
            tri[v][axis] = f64::from(value);
        }
    }
    Ok(tri)
}

fn unique_push(points: &mut [[f64; 2]; 6], n: &mut usize, p: [f64; 2]) {
    if !(0..*n).any(|i| points[i][0] == p[0] && points[i][1] == p[1]) {
        points[*n] = p;
        *n += 1;
    }
}

/// Interseccion exacta topologicamente para los valores f64 decodificados,
/// con interpolacion aritmetica f64; sin umbral de proximidad ni muestreo.
/// Puede devolver varias aristas de un triangulo coplanar.
pub fn triangle_section_y(tri: [[f64; 3]; 3], y: f64) -> Vec<SectionPiece> {
    let ys = [tri[0][1], tri[1][1], tri[2][1]];
    if ys.iter().all(|v| *v == y) {
        let mut output = Vec::with_capacity(3);
        for i in 0..3 {
            let a = [tri[i][0], tri[i][2]];
            let j = (i + 1) % 3;
            let b = [tri[j][0], tri[j][2]];
            if a != b {
                output.push(SectionPiece {
                    a,
                    b,
                    kind: PieceKind::CoplanarEdge,
                });
            }
        }
        return output;
    }
    let mut points = [[0f64; 2]; 6];
    let mut n = 0;
    for i in 0..3 {
        let j = (i + 1) % 3;
        let a = tri[i];
        let b = tri[j];
        let da = a[1] - y;
        let db = b[1] - y;
        if da == 0.0 {
            unique_push(&mut points, &mut n, [a[0], a[2]]);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = (y - a[1]) / (b[1] - a[1]);
            unique_push(
                &mut points,
                &mut n,
                [a[0] + t * (b[0] - a[0]), a[2] + t * (b[2] - a[2])],
            );
        }
    }
    match n {
        0 => vec![],
        1 => vec![SectionPiece {
            a: points[0],
            b: points[0],
            kind: PieceKind::Point,
        }],
        _ => {
            // Ante triangulos degenerados, elegir el par mas separado evita
            // conectar componentes de triangulos distintos por proximidad.
            let mut best = (0, 1);
            let mut max_d2 = -1f64;
            for i in 0..n {
                for j in (i + 1)..n {
                    let dx = points[i][0] - points[j][0];
                    let dz = points[i][1] - points[j][1];
                    let d2 = dx * dx + dz * dz;
                    if d2 > max_d2 {
                        max_d2 = d2;
                        best = (i, j);
                    }
                }
            }
            if max_d2 == 0.0 {
                vec![SectionPiece {
                    a: points[best.0],
                    b: points[best.0],
                    kind: PieceKind::Point,
                }]
            } else {
                vec![SectionPiece {
                    a: points[best.0],
                    b: points[best.1],
                    kind: PieceKind::Segment,
                }]
            }
        }
    }
}

/// Lee el archivo dos veces sobre el mismo manejador, manteniendo memoria
/// de trabajo acotada, y emite elementos sin unirlos artificialmente.
/// `fractions` son proporciones (0,1) de la extension Y observada del STL.
pub fn extract_y_sections<S: SectionSink>(
    path: &Path,
    fractions: &[f64],
    max_records: u64,
    cancel: Option<&Path>,
    sink: &mut S,
) -> Result<SectionSummary, SectionError> {
    if fractions.is_empty()
        || fractions.len() > 16
        || fractions
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v >= 1.0)
    {
        return Err(SectionError::InvalidInput(
            "1..16 secciones y fracciones estrictamente entre 0 y 1",
        ));
    }
    if max_records == 0 {
        return Err(SectionError::InvalidInput("max_records debe ser positivo"));
    }
    check_cancel(cancel)?;
    let mut reader = protected_reader(path)?;
    let original_meta = reader.get_ref().metadata()?;
    if !original_meta.is_file() || original_meta.len() < STL_HEADER {
        return Err(SectionError::InvalidStl("archivo corto o no regular"));
    }
    let mut header = [0u8; STL_HEADER as usize];
    reader.read_exact(&mut header)?;
    let count = u32::from_le_bytes(header[80..84].try_into().unwrap());
    if count == 0 || STL_HEADER + u64::from(count) * STL_RECORD != original_meta.len() {
        return Err(SectionError::InvalidStl(
            "longitud y recuento de triangulos incoherentes",
        ));
    }
    let mut record = [0u8; STL_RECORD as usize];
    let mut bounds = Bounds3::empty();
    for i in 0..count {
        if i % CANCEL_PERIOD == 0 {
            check_cancel(cancel)?;
        }
        reader.read_exact(&mut record)?;
        bounds.include(&triangle_from_record(&record, i)?);
    }
    let span_y = bounds.max[1] - bounds.min[1];
    if span_y <= 0.0 {
        return Err(SectionError::InvalidStl("extension Y nula"));
    }
    let planes_y: Vec<_> = fractions
        .iter()
        .map(|f| bounds.min[1] + span_y * f)
        .collect();
    sink.start(&bounds, &planes_y)?;
    reader.seek(SeekFrom::Start(STL_HEADER))?;
    let mut summary = SectionSummary {
        bounds,
        triangles: count,
        planes_y,
        records_per_plane: vec![0; fractions.len()],
        isolated_points: 0,
        coplanar_triangles: 0,
        total_records: 0,
    };
    for i in 0..count {
        if i % CANCEL_PERIOD == 0 {
            check_cancel(cancel)?;
        }
        reader.read_exact(&mut record)?;
        let tri = triangle_from_record(&record, i)?;
        let y0 = tri.iter().fold(f64::INFINITY, |m, p| m.min(p[1]));
        let y1 = tri.iter().fold(f64::NEG_INFINITY, |m, p| m.max(p[1]));
        for (s, &y) in summary.planes_y.iter().enumerate() {
            if y < y0 || y > y1 {
                continue;
            }
            let pieces = triangle_section_y(tri, y);
            if tri.iter().all(|v| v[1] == y) && !pieces.is_empty() {
                summary.coplanar_triangles += 1;
            }
            for piece in pieces {
                if summary.total_records >= max_records {
                    return Err(SectionError::LimitExceeded(max_records));
                }
                if piece.kind == PieceKind::Point {
                    summary.isolated_points += 1;
                }
                sink.piece(s, piece)?;
                summary.total_records += 1;
                summary.records_per_plane[s] += 1;
            }
        }
    }
    check_cancel(cancel)?;
    let final_meta = reader.get_ref().metadata()?;
    if original_meta.len() != final_meta.len()
        || original_meta.modified().ok() != final_meta.modified().ok()
    {
        return Err(SectionError::ChangedDuringRead);
    }
    sink.finish(&summary)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn has_endpoints(p: SectionPiece, a: [f64; 2], b: [f64; 2]) -> bool {
        (p.a == a && p.b == b) || (p.a == b && p.b == a)
    }
    #[test]
    fn plano_horizontal_corta_sin_herramienta() {
        let tr = [[0., 0., 5.], [2., 0., 5.], [0., 1., 5.]];
        let p = triangle_section_y(tr, 0.25);
        assert_eq!(p.len(), 1);
        assert!(has_endpoints(p[0], [0., 5.], [1.5, 5.]));
    }
    #[test]
    fn triangulo_inclinado_interpolado() {
        let tr = [[0., 0., 0.], [2., 0., 2.], [0., 2., 4.]];
        let p = triangle_section_y(tr, 1.);
        assert_eq!(p.len(), 1);
        assert!(has_endpoints(p[0], [0., 2.], [1., 3.]));
    }
    #[test]
    fn pared_vertical_no_se_aplana() {
        let tr = [[0., 0., 0.], [0., 2., 0.], [0., 2., 6.]];
        let p = triangle_section_y(tr, 1.);
        assert_eq!(p.len(), 1);
        assert!(has_endpoints(p[0], [0., 0.], [0., 3.]));
    }
    #[test]
    fn contacto_tangencial_conserva_punto_aislado() {
        let tr = [[0., 0., 5.], [1., 1., 6.], [-1., 1., 4.]];
        let p = triangle_section_y(tr, 0.);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].kind, PieceKind::Point);
        assert_eq!(p[0].a, [0., 5.]);
    }
    #[test]
    fn coplanar_conserva_aristas() {
        let tr = [[0., 1., 0.], [1., 1., 0.], [0., 1., 2.]];
        let p = triangle_section_y(tr, 1.);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|p| p.kind == PieceKind::CoplanarEdge));
    }
    #[test]
    fn multicapas_no_se_funden_por_x() {
        let bottom = [[0., 0., 1.], [2., 0., 1.], [0., 2., 1.]];
        let top = [[0., 0., 9.], [2., 0., 9.], [0., 2., 9.]];
        let a = triangle_section_y(bottom, 1.);
        let b = triangle_section_y(top, 1.);
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].a[1], 1.);
        assert_eq!(b[0].a[1], 9.);
    }
    #[test]
    fn fuera_de_plano_no_hay_segmentos() {
        let tr = [[0., 0., 0.], [1., 1., 0.], [0., 2., 1.]];
        assert!(triangle_section_y(tr, 3.).is_empty());
    }
    struct Sink(Vec<(usize, SectionPiece)>);
    impl SectionSink for Sink {
        fn start(&mut self, _: &Bounds3, ys: &[f64]) -> io::Result<()> {
            assert_eq!(ys.len(), 1);
            Ok(())
        }
        fn piece(&mut self, i: usize, p: SectionPiece) -> io::Result<()> {
            self.0.push((i, p));
            Ok(())
        }
    }
    #[test]
    fn extremo_a_extremo_stl_binario() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let name = format!(
            "lagucam_section_test_{}_{}.stl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        let triangle = [[0f32, 0., 5.], [2., 0., 5.], [0., 1., 5.]];
        let mut bytes = vec![0u8; 84];
        bytes[80..84].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 12]);
        for v in triangle {
            for a in v {
                bytes.extend_from_slice(&a.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
        std::fs::write(&path, &bytes).unwrap();
        let mut sink = Sink(Vec::new());
        let res = extract_y_sections(&path, &[0.25], 100, None, &mut sink).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(res.triangles, 1);
        assert_eq!(res.total_records, 1);
        assert_eq!(res.planes_y, vec![0.25]);
        assert!(has_endpoints(sink.0[0].1, [0., 5.], [1.5, 5.]));
    }
}

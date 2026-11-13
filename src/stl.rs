//! STL binario estricto en dos pasadas. El archivo original solo se abre para lectura.
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

pub const RECORD_SIZE: u64 = 50;
pub const CANCELLED: &str = "Importación cancelada";

pub fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    fn add(&mut self, p: [f32; 3]) {
        for (a, value) in p.into_iter().enumerate() {
            self.min[a] = self.min[a].min(value as f64);
            self.max[a] = self.max[a].max(value as f64);
        }
    }
    pub fn dimensions(&self) -> [f64; 3] {
        std::array::from_fn(|i| self.max[i] - self.min[i])
    }
    pub fn center(&self) -> [f64; 3] {
        std::array::from_fn(|i| self.min[i] + (self.max[i] - self.min[i]) * 0.5)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StlInfo {
    pub triangles: u32,
    pub size_bytes: u64,
    pub bounds: Bounds,
}

fn parse_triangle(bytes: &[u8; 50]) -> Result<[[f32; 3]; 3], String> {
    let mut vertices = [[0.0; 3]; 3];
    // Las normales almacenadas y los atributos no cambian las posiciones.
    for (v, vertex) in vertices.iter_mut().enumerate() {
        for (axis, value) in vertex.iter_mut().enumerate() {
            let offset = 12 + v * 12 + axis * 4;
            *value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            if !value.is_finite() {
                return Err("STL con coordenada NaN o infinita".into());
            }
        }
    }
    Ok(vertices)
}

/// Callback por triángulo, sin retener una segunda copia completa de la geometría.
/// No proporciona una instantánea inmutable frente a modificaciones externas.
pub fn scan<F>(
    path: &Path,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    consume: F,
) -> Result<StlInfo, String>
where
    F: FnMut([[f32; 3]; 3], &Bounds) -> Result<(), String>,
{
    check_cancel(cancel)?;
    let file = File::open(path).map_err(|e| format!("No se puede abrir el fichero: {e}"))?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err("La ruta no corresponde a un archivo regular".into());
    }
    let mut reader = BufReader::with_capacity(4 * 1024 * 1024, file);
    let info = scan_reader(&mut reader, before.len(), progress, cancel, consume)?;
    let after = reader.get_ref().metadata().map_err(|e| e.to_string())?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("El archivo cambió durante la lectura; vuelva a abrirlo".into());
    }
    Ok(info)
}

fn scan_reader<R, F>(
    reader: &mut R,
    size_bytes: u64,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    mut consume: F,
) -> Result<StlInfo, String>
where
    R: Read + Seek,
    F: FnMut([[f32; 3]; 3], &Bounds) -> Result<(), String>,
{
    check_cancel(cancel)?;
    if size_bytes < 84 {
        return Err("STL demasiado pequeño (se espera STL binario)".into());
    }
    let mut header = [0; 84];
    reader.read_exact(&mut header).map_err(|e| e.to_string())?;
    let triangles = u32::from_le_bytes(header[80..84].try_into().unwrap());
    let expected = 84 + u64::from(triangles) * RECORD_SIZE;
    if expected != size_bytes {
        return Err(format!(
            "Tamaño incoherente: {triangles} triángulos requieren {expected} bytes, archivo de {size_bytes} bytes"
        ));
    }
    if triangles == 0 {
        return Err("STL vacío".into());
    }
    let mut record = [0; 50];
    let mut bounds = Bounds::empty();
    for i in 0..triangles {
        check_cancel(cancel)?;
        reader
            .read_exact(&mut record)
            .map_err(|e| format!("STL truncado: {e}"))?;
        for p in parse_triangle(&record)? {
            bounds.add(p);
        }
        if i % 16_384 == 0 {
            progress.store(
                (u64::from(i) * 40 / u64::from(triangles)) as u32,
                Ordering::Relaxed,
            );
        }
    }
    // Un plano vertical es válido. Solo una extensión nula en TODOS los ejes impide encuadrar.
    if bounds.dimensions().into_iter().fold(0.0_f64, f64::max) == 0.0 {
        return Err("La geometría no tiene extensión espacial".into());
    }
    reader
        .seek(SeekFrom::Start(84))
        .map_err(|e| e.to_string())?;
    progress.store(40, Ordering::Relaxed);
    for i in 0..triangles {
        check_cancel(cancel)?;
        reader
            .read_exact(&mut record)
            .map_err(|e| format!("STL truncado: {e}"))?;
        consume(parse_triangle(&record)?, &bounds)?;
        if i % 16_384 == 0 {
            progress.store(
                40 + (u64::from(i) * 55 / u64::from(triangles)) as u32,
                Ordering::Relaxed,
            );
        }
    }
    check_cancel(cancel)?;
    progress.store(96, Ordering::Relaxed);
    Ok(StlInfo {
        triangles,
        size_bytes,
        bounds,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;

    pub fn binary(triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
        let mut bytes = vec![0; 80];
        bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for triangle in triangles {
            bytes.extend_from_slice(&[0; 12]);
            for p in triangle {
                for value in p {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
            bytes.extend_from_slice(&[0; 2]);
        }
        bytes
    }
    fn read(bytes: Vec<u8>) -> Result<StlInfo, String> {
        let size = bytes.len() as u64;
        scan_reader(
            &mut Cursor::new(bytes),
            size,
            &AtomicU32::new(0),
            &AtomicBool::new(false),
            |_, _| Ok(()),
        )
    }
    const TRI: [[f32; 3]; 3] = [[0., 0., 0.], [0., 1., 0.], [0., 0., 1.]];

    #[test]
    fn parses_little_endian_vertices() {
        let bytes = binary(&[TRI]);
        assert_eq!(
            parse_triangle(bytes[84..].try_into().unwrap()).unwrap(),
            TRI
        );
    }
    #[test]
    fn rejects_non_finite_geometry() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut tri = TRI;
            tri[1][0] = value;
            assert!(read(binary(&[tri])).is_err());
        }
    }
    #[test]
    fn accepts_vertical_plane_and_solid_header() {
        let mut bytes = binary(&[TRI]);
        bytes[..5].copy_from_slice(b"solid");
        assert_eq!(read(bytes).unwrap().bounds.dimensions(), [0., 1., 1.]);
    }
    #[test]
    fn rejects_empty_short_truncated_trailing_and_wrong_count() {
        for bytes in [
            vec![],
            vec![0; 83],
            binary(&[]),
            binary(&[TRI])[..100].to_vec(),
        ] {
            assert!(read(bytes).is_err());
        }
        let mut bytes = binary(&[TRI]);
        bytes.push(0);
        assert!(read(bytes).is_err());
        let mut bytes = binary(&[TRI]);
        bytes[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(bytes).is_err());
    }
    #[test]
    fn rejects_zero_extent_but_handles_extreme_finite_coordinates() {
        assert!(read(binary(&[[[1.; 3]; 3]])).is_err());
        let info = read(binary(&[[
            [-f32::MAX, 0., 0.],
            [f32::MAX, 0., 0.],
            [0., 1., 0.],
        ]]))
        .unwrap();
        assert!(info.bounds.dimensions()[0].is_finite());
    }
    #[test]
    fn cancellation_before_and_during_second_pass() {
        let bytes = binary(&[TRI, TRI]);
        let size = bytes.len() as u64;
        let cancel = AtomicBool::new(true);
        assert_eq!(
            scan_reader(
                &mut Cursor::new(&bytes),
                size,
                &AtomicU32::new(0),
                &cancel,
                |_, _| Ok(())
            )
            .unwrap_err(),
            CANCELLED
        );
        cancel.store(false, Ordering::Relaxed);
        let mut seen = 0;
        let result = scan_reader(
            &mut Cursor::new(&bytes),
            size,
            &AtomicU32::new(0),
            &cancel,
            |_, _| {
                seen += 1;
                cancel.store(true, Ordering::Relaxed);
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert_eq!(seen, 1);
    }
    #[test]
    fn cancellation_interrupts_first_pass() {
        struct CancelReader<'a> {
            data: Cursor<Vec<u8>>,
            cancel: &'a AtomicBool,
        }
        impl Read for CancelReader<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let n = self.data.read(out)?;
                if self.data.position() >= 134 {
                    self.cancel.store(true, Ordering::Relaxed);
                }
                Ok(n)
            }
        }
        impl Seek for CancelReader<'_> {
            fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
                self.data.seek(pos)
            }
        }
        let bytes = binary(&[TRI, TRI]);
        let size = bytes.len() as u64;
        let cancel = AtomicBool::new(false);
        let mut reader = CancelReader {
            data: Cursor::new(bytes),
            cancel: &cancel,
        };
        let result = scan_reader(&mut reader, size, &AtomicU32::new(0), &cancel, |_, _| {
            panic!("No debe llegar a la segunda pasada")
        });
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert_eq!(reader.data.position(), 134);
    }

    #[test]
    fn propagates_consumer_error_and_actual_read_failure() {
        let bytes = binary(&[TRI]);
        let size = bytes.len() as u64;
        assert_eq!(
            scan_reader(
                &mut Cursor::new(&bytes),
                size,
                &AtomicU32::new(0),
                &AtomicBool::new(false),
                |_, _| Err("consumer".into())
            )
            .unwrap_err(),
            "consumer"
        );
        assert!(
            scan_reader(
                &mut Cursor::new(&bytes[..100]),
                size,
                &AtomicU32::new(0),
                &AtomicBool::new(false),
                |_, _| Ok(())
            )
            .is_err()
        );
    }
}

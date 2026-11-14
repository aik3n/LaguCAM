//! Formato de proyecto independiente de egui, de los hilos y de OpenGL.
//! Las coordenadas originales se guardan como f64 en JSON; el STL sigue externo.
use crate::{
    project::{
        Project,
        display::{StockColor, StockDisplay, StockMode},
        operations::{Operation, OperationKind, OperationStatus, Operations},
        origin::{OriginPoint, OriginReference, WorkOrigin},
        selection::Selection,
        stock::Stock,
    },
    stl::{Bounds, StlInfo},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const FORMAT_VERSION: u32 = 1;
const MAX_PROJECT_FILE: u64 = 16 * 1024 * 1024;
const SAMPLE_LENGTH: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocument {
    pub format: String,
    pub version: u32,
    pub units: String,
    pub stl: Option<StlReference>,
    pub state: ProjectState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StlReference {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub triangles: u32,
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    /// FNV-1a de cinco muestras de hasta 64 KiB: barato, no criptográfico.
    pub sample_fnv64: String,
    /// Marca temporal informativa; puede cambiar al copiar el STL.
    pub modified_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectState {
    pub stock: Option<SavedStock>,
    pub stock_display: SavedDisplay,
    pub origin: SavedOrigin,
    pub model_color: [u8; 3],
    pub operations: Vec<SavedOperation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedStock {
    pub min: [f64; 3],
    pub size: [f64; 3],
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDisplay {
    pub mode: String,
    pub color: String,
    pub opacity_percent: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOrigin {
    pub reference: String,
    pub point: SavedPoint,
    pub offset: [f64; 3],
    pub rotation_degrees: [f64; 3],
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "index", rename_all = "snake_case")]
pub enum SavedPoint {
    Corner(u8),
    TopCenter,
    Center,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOperation {
    pub name: String,
    pub kind: String,
    pub status: String,
    pub tool_id: Option<u64>,
    pub toolpath_id: Option<u64>,
}

fn valid_bounds(min: [f64; 3], max: [f64; 3]) -> bool {
    min.iter()
        .zip(max)
        .all(|(a, b)| a.is_finite() && b.is_finite() && b >= *a && (b - *a).is_finite())
}

fn same_f64(a: [f64; 3], b: [f64; 3]) -> bool {
    a.into_iter()
        .zip(b)
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

impl ProjectState {
    /// Proyección de los datos de negocio; no incluye selección, cámara ni GPU.
    pub fn capture(project: &Project) -> Self {
        let stock = project.stock.as_ref().map(|s| SavedStock {
            min: s.min,
            size: s.size,
            visible: s.visible,
        });
        let display = project.stock_display;
        let origin = &project.origin;
        Self {
            stock,
            stock_display: SavedDisplay {
                mode: match display.mode {
                    StockMode::Solid => "solid",
                    StockMode::Edges => "edges",
                    StockMode::SolidEdges => "solid_edges",
                }
                .into(),
                color: match display.color {
                    StockColor::Amber => "amber",
                    StockColor::Blue => "blue",
                    StockColor::Gray => "gray",
                    StockColor::Green => "green",
                }
                .into(),
                opacity_percent: display.opacity_percent,
            },
            origin: SavedOrigin {
                reference: match origin.reference {
                    OriginReference::Stock => "stock",
                    OriginReference::Model => "model",
                }
                .into(),
                point: match origin.point {
                    OriginPoint::Corner(i) => SavedPoint::Corner(i),
                    OriginPoint::TopCenter => SavedPoint::TopCenter,
                    OriginPoint::Center => SavedPoint::Center,
                },
                offset: origin.offset,
                rotation_degrees: origin.rotation_degrees,
                visible: origin.visible,
            },
            model_color: project.model_color,
            operations: project
                .operations
                .items
                .iter()
                .map(|op| SavedOperation {
                    name: op.name.clone(),
                    kind: match &op.kind {
                        OperationKind::Placeholder => "placeholder",
                    }
                    .into(),
                    status: match &op.status {
                        OperationStatus::NotCalculated => "not_calculated",
                        OperationStatus::Ready => "ready",
                        OperationStatus::Error => "error",
                    }
                    .into(),
                    tool_id: op.tool_id,
                    toolpath_id: op.toolpath_id,
                })
                .collect(),
        }
    }

    pub fn validate(&self, model_bounds: Option<Bounds>) -> Result<(), String> {
        if self.stock_display.opacity_percent > 100 {
            return Err("Opacidad del bruto fuera del rango 0..100".into());
        }
        if !["solid", "edges", "solid_edges"].contains(&self.stock_display.mode.as_str()) {
            return Err("Modo de visualización desconocido".into());
        }
        if !["amber", "blue", "gray", "green"].contains(&self.stock_display.color.as_str()) {
            return Err("Color del bruto desconocido".into());
        }
        let stock = self.stock.as_ref().map(|s| Stock {
            min: s.min,
            size: s.size,
            visible: s.visible,
        });
        if let Some(stock) = &stock {
            stock.validate().map_err(str::to_owned)?;
        }
        let origin = self.to_origin()?;
        origin.rotation_basis().map_err(str::to_owned)?;
        if !origin.offset.iter().all(|v| v.is_finite()) {
            return Err("Offset CNC no finito".into());
        }
        if let (Some(stock), Some(bounds)) = (&stock, model_bounds) {
            origin
                .model_position(stock, &bounds)
                .map_err(str::to_owned)?;
        } else if stock.is_some() || model_bounds.is_some() {
            return Err("El bruto y el modelo deben existir juntos".into());
        }
        for op in &self.operations {
            if op.name.is_empty() || op.name.len() > 4096 {
                return Err("Nombre de operación inválido".into());
            }
            if op.kind != "placeholder"
                || !["not_calculated", "ready", "error"].contains(&op.status.as_str())
            {
                return Err("Tipo o estado de operación incompatible".into());
            }
        }
        Ok(())
    }

    fn to_origin(&self) -> Result<WorkOrigin, String> {
        let reference = match self.origin.reference.as_str() {
            "stock" => OriginReference::Stock,
            "model" => OriginReference::Model,
            _ => return Err("Referencia del origen CNC desconocida".into()),
        };
        let point = match self.origin.point {
            SavedPoint::Corner(i) if i < 8 => OriginPoint::Corner(i),
            SavedPoint::Corner(_) => return Err("Esquina CNC fuera del rango 0..7".into()),
            SavedPoint::TopCenter => OriginPoint::TopCenter,
            SavedPoint::Center => OriginPoint::Center,
        };
        Ok(WorkOrigin {
            reference,
            point,
            offset: self.origin.offset,
            rotation_degrees: self.origin.rotation_degrees,
            visible: self.origin.visible,
        })
    }

    pub fn restore(&self, name: String, info: Option<StlInfo>) -> Result<Project, String> {
        self.validate(info.map(|i| i.bounds))?;
        let mut project = Project::default();
        if let Some(info) = info {
            project.loaded(name, info);
        }
        project.stock = self.stock.as_ref().map(|s| Stock {
            min: s.min,
            size: s.size,
            visible: s.visible,
        });
        project.stock_display = StockDisplay {
            mode: match self.stock_display.mode.as_str() {
                "solid" => StockMode::Solid,
                "edges" => StockMode::Edges,
                _ => StockMode::SolidEdges,
            },
            color: match self.stock_display.color.as_str() {
                "amber" => StockColor::Amber,
                "blue" => StockColor::Blue,
                "gray" => StockColor::Gray,
                _ => StockColor::Green,
            },
            opacity_percent: self.stock_display.opacity_percent,
        };
        project.model_color = self.model_color;
        project.origin = self.to_origin()?;
        project.operations = Operations {
            items: self
                .operations
                .iter()
                .map(|op| Operation {
                    name: op.name.clone(),
                    kind: OperationKind::Placeholder,
                    status: match op.status.as_str() {
                        "ready" => OperationStatus::Ready,
                        "error" => OperationStatus::Error,
                        _ => OperationStatus::NotCalculated,
                    },
                    tool_id: op.tool_id,
                    toolpath_id: op.toolpath_id,
                })
                .collect(),
        };
        project.selection = Selection::Model;
        Ok(project)
    }
}

impl ProjectDocument {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "LaguCAM" {
            return Err("No es un proyecto LaguCAM".into());
        }
        if self.version != FORMAT_VERSION {
            return Err(format!(
                "Versión .lagucam incompatible: {} (soportada: {})",
                self.version, FORMAT_VERSION
            ));
        }
        if self.units != "mm" {
            return Err("Unidades de proyecto no soportadas (solo mm)".into());
        }
        let bounds = self.stl.as_ref().map(|s| Bounds {
            min: s.bounds_min,
            max: s.bounds_max,
        });
        if let Some(stl) = &self.stl
            && (stl.path.as_os_str().is_empty()
                || stl.triangles == 0
                || stl.size_bytes != 84 + u64::from(stl.triangles) * 50
                || !valid_bounds(stl.bounds_min, stl.bounds_max)
                || stl.sample_fnv64.len() != 16
                || !stl.sample_fnv64.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Identidad STL inválida en el proyecto".into());
        }
        self.state.validate(bounds)
    }

    pub fn from_project(
        project: &Project,
        path: Option<&Path>,
        project_file: &Path,
    ) -> Result<Self, String> {
        let state = ProjectState::capture(project);
        let stl = match (&project.model, path) {
            (Some(model), Some(path)) => {
                let (size, count, signature, modified) = sample_identity(path)?;
                if size != model.size_bytes || count != model.triangles {
                    return Err("El STL cambió desde su importación; no se guarda una referencia incorrecta".into());
                }
                Some(StlReference {
                    path: make_relative(path, project_file),
                    size_bytes: size,
                    triangles: count,
                    bounds_min: model.bounds.min,
                    bounds_max: model.bounds.max,
                    sample_fnv64: signature,
                    modified_unix_ms: modified,
                })
            }
            (None, None) => None,
            _ => return Err("Referencia STL incompleta".into()),
        };
        let doc = Self {
            format: "LaguCAM".into(),
            version: FORMAT_VERSION,
            units: "mm".into(),
            stl,
            state,
        };
        doc.validate()?;
        Ok(doc)
    }

    pub fn resolve_stl(&self, project_file: &Path) -> Option<PathBuf> {
        self.stl.as_ref().map(|s| {
            if s.path.is_absolute() {
                s.path.clone()
            } else {
                project_file
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(&s.path)
            }
        })
    }

    pub fn check_stl(&self, path: &Path) -> Result<Option<String>, String> {
        let expected = self.stl.as_ref().ok_or("El proyecto no contiene STL")?;
        let (size, count, hash, modified) = sample_identity(path)?;
        if expected.size_bytes != size
            || expected.triangles != count
            || expected.sample_fnv64 != hash
        {
            return Err("El STL no coincide con el guardado (tamaño, triángulos o muestras); se mantiene el proyecto anterior".into());
        }
        if expected.modified_unix_ms != modified {
            Ok(Some("La fecha del STL cambió, pero las muestras coinciden; se verificarán sus límites al cargar".into()))
        } else {
            Ok(None)
        }
    }

    pub fn check_loaded(&self, info: StlInfo) -> Result<(), String> {
        let expected = self.stl.as_ref().ok_or("Proyecto sin STL")?;
        if expected.size_bytes != info.size_bytes
            || expected.triangles != info.triangles
            || !same_f64(expected.bounds_min, info.bounds.min)
            || !same_f64(expected.bounds_max, info.bounds.max)
        {
            return Err(
                "Los límites o metadatos reales del STL difieren: configuración no aplicada".into(),
            );
        }
        Ok(())
    }
}

/// Firma económica: ~320 KiB de lectura como máximo por comprobación, sin hash de 1 GiB.
fn sample_identity(path: &Path) -> Result<(u64, u32, String, Option<u64>), String> {
    let mut file =
        File::open(path).map_err(|e| format!("No se puede leer STL {}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    let size = metadata.len();
    if !metadata.is_file() || size < 84 {
        return Err("STL inexistente, vacío o inválido".into());
    }
    let mut head = [0; 84];
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    let triangles = u32::from_le_bytes(
        head[80..84]
            .try_into()
            .map_err(|_| "Cabecera STL incorrecta")?,
    );
    if triangles == 0 || size != 84 + u64::from(triangles) * 50 {
        return Err("Cabecera o longitud STL incorrecta".into());
    }
    let width = SAMPLE_LENGTH as u64;
    let locations = [
        0,
        size / 4,
        size / 2,
        size.saturating_mul(3) / 4,
        size.saturating_sub(width),
    ];
    let mut digest = 0xcbf29ce484222325_u64;
    for location in locations {
        file.seek(SeekFrom::Start(location))
            .map_err(|e| e.to_string())?;
        let count = (size - location).min(width) as usize;
        let mut sample = vec![0u8; count];
        file.read_exact(&mut sample).map_err(|e| e.to_string())?;
        for b in location.to_le_bytes().into_iter().chain(sample) {
            digest = (digest ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    let after = file.metadata().map_err(|e| e.to_string())?;
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err("El STL cambió durante la comprobación".into());
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .and_then(|v| u64::try_from(v.as_millis()).ok());
    Ok((size, triangles, format!("{digest:016x}"), modified))
}

/// Devuelve ruta relativa siempre que origen y proyecto compartan un volumen.
fn make_relative(source: &Path, project_file: &Path) -> PathBuf {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let base = project_file.parent().unwrap_or_else(|| Path::new("."));
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let left: Vec<_> = source.components().collect();
    let right: Vec<_> = base.components().collect();
    if left.first() != right.first() || left.is_empty() || right.is_empty() {
        return source;
    }
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    let mut path = PathBuf::new();
    for part in &right[common..] {
        if matches!(part, Component::Normal(_)) {
            path.push("..");
        } else {
            return source;
        }
    }
    for part in &left[common..] {
        path.push(part.as_os_str());
    }
    if path.as_os_str().is_empty() {
        source
    } else {
        path
    }
}

pub fn read_project(path: &Path) -> Result<ProjectDocument, String> {
    let size = fs::metadata(path)
        .map_err(|e| format!("No se encuentra el proyecto: {e}"))?
        .len();
    if size > MAX_PROJECT_FILE {
        return Err("Proyecto demasiado grande (máximo 16 MiB)".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("No se puede leer el proyecto: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON .lagucam inválido: {e}"))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Proyecto sin versión válida")?;
    if version != u64::from(FORMAT_VERSION) {
        return Err(format!(
            "Versión .lagucam incompatible: {version} (soportada: {FORMAT_VERSION})"
        ));
    }
    let doc: ProjectDocument =
        serde_json::from_value(value).map_err(|e| format!("Datos .lagucam inválidos: {e}"))?;
    doc.validate()?;
    Ok(doc)
}

pub fn write_project(path: &Path, doc: &ProjectDocument) -> Result<(), String> {
    write_project_with(path, doc, replace_file)
}

fn write_project_with<F>(path: &Path, doc: &ProjectDocument, replace: F) -> Result<(), String>
where
    F: FnOnce(&Path, &Path, u128) -> Result<(), String>,
{
    doc.validate()?;
    let data = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("Ruta de proyecto sin carpeta")?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    if !parent.is_dir() {
        return Err("No existe el directorio de destino".into());
    }
    let name = path
        .file_name()
        .ok_or("Nombre de proyecto vacío")?
        .to_string_lossy();
    let stamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let tmp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), stamp));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("No se puede crear archivo temporal: {e}"))?;
        file.write_all(&data)
            .map_err(|e| format!("Error al escribir temporal: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("No se pudo sincronizar proyecto: {e}"))?;
        drop(file);
        replace(&tmp, path, stamp)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(windows)]
fn replace_file(tmp: &Path, target: &Path, stamp: u128) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        #[link_name = "ReplaceFileW"]
        fn replace_file_w(
            old_name: *const u16,
            new_name: *const u16,
            backup_name: *const u16,
            flags: u32,
            exclude: *const std::ffi::c_void,
            reserved: *const std::ffi::c_void,
        ) -> i32;
    }
    if !target.exists() {
        return fs::rename(tmp, target).map_err(|e| e.to_string());
    }
    let backup = target.with_extension(format!("lagucam.recovery.{stamp}.bak"));
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let old = wide(target);
    let new = wide(tmp);
    let bak = wide(&backup);
    // SAFETY: punteros UTF-16 terminados en cero; buffers viven durante la llamada síncrona.
    if unsafe {
        replace_file_w(
            old.as_ptr(),
            new.as_ptr(),
            bak.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(format!(
            "No se pudo reemplazar el proyecto: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, target: &Path, _stamp: u128) -> Result<(), String> {
    fs::rename(tmp, target).map_err(|e| format!("Error en renombrado atómico: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::operations::{Operation, OperationKind, OperationStatus};
    use std::sync::atomic::{AtomicU64, Ordering};
    static RUN: AtomicU64 = AtomicU64::new(0);

    fn workspace() -> PathBuf {
        let n = RUN.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lagucam_persist_test_{}_{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn test_stl(path: &Path) -> StlInfo {
        let tris = [[[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 2.0]]];
        fs::write(path, crate::stl::tests::binary(&tris)).unwrap();
        StlInfo {
            triangles: 1,
            size_bytes: 134,
            bounds: Bounds {
                min: [0.0; 3],
                max: [1.0, 1.0, 2.0],
            },
        }
    }
    fn configured(info: StlInfo) -> Project {
        let mut project = Project::default();
        project.loaded("modelo.stl".into(), info);
        project.stock = Some(Stock {
            min: [-1.125, -0.25, -1.0],
            size: [4.25, 6.75, 8.875],
            visible: false,
        });
        project.stock_display = StockDisplay {
            mode: StockMode::SolidEdges,
            color: StockColor::Blue,
            opacity_percent: 62,
        };
        project.model_color = [12, 220, 33];
        project.origin = WorkOrigin {
            reference: OriginReference::Model,
            point: OriginPoint::Corner(7),
            offset: [-0.125, 0.25, 1.23456789012345],
            rotation_degrees: [30.0, -45.25, 12.75],
            visible: false,
        };
        project.operations.items = vec![
            Operation {
                name: "Primera".into(),
                kind: OperationKind::Placeholder,
                status: OperationStatus::Ready,
                tool_id: Some(2),
                toolpath_id: None,
            },
            Operation {
                name: "Segunda".into(),
                kind: OperationKind::Placeholder,
                status: OperationStatus::Error,
                tool_id: None,
                toolpath_id: Some(7),
            },
        ];
        project
    }
    #[test]
    fn roundtrip_all_settings_and_exact_f64() {
        let dir = workspace();
        let stl = dir.join("modelo.stl");
        let info = test_stl(&stl);
        let project = configured(info);
        let file = dir.join("ejemplo.lagucam");
        let doc = ProjectDocument::from_project(&project, Some(&stl), &file).unwrap();
        write_project(&file, &doc).unwrap();
        let restored = read_project(&file).unwrap();
        assert_eq!(doc, restored);
        assert_eq!(restored.units, "mm");
        assert_eq!(
            restored.stl.as_ref().unwrap().path.as_path(),
            Path::new("modelo.stl")
        );
        let reloaded = restored
            .state
            .restore("modelo.stl".into(), Some(info))
            .unwrap();
        assert_eq!(
            ProjectState::capture(&reloaded),
            ProjectState::capture(&project)
        );
        for (a, b) in reloaded
            .stock
            .unwrap()
            .min
            .into_iter()
            .zip(project.stock.unwrap().min)
        {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn relative_move_and_modified_stl_detection() {
        let dir = workspace();
        let sub = dir.join("saved");
        fs::create_dir_all(&sub).unwrap();
        let stl = dir.join("modelo.stl");
        let info = test_stl(&stl);
        let file = sub.join("p.lagucam");
        let doc = ProjectDocument::from_project(&configured(info), Some(&stl), &file).unwrap();
        assert!(doc.stl.as_ref().unwrap().path.is_relative());
        assert_eq!(
            doc.resolve_stl(&file).unwrap().canonicalize().unwrap(),
            stl.canonicalize().unwrap()
        );
        let relocated = dir.join("another_location");
        fs::create_dir_all(relocated.join("saved")).unwrap();
        fs::copy(&stl, relocated.join("modelo.stl")).unwrap();
        let relocated_file = relocated.join("saved").join("p.lagucam");
        assert_eq!(
            doc.resolve_stl(&relocated_file)
                .unwrap()
                .canonicalize()
                .unwrap(),
            relocated.join("modelo.stl").canonicalize().unwrap()
        );
        assert!(doc.check_stl(&stl).is_ok());
        let bytes = fs::read(&stl).unwrap();
        let mut changed = bytes.clone();
        changed[4] ^= 0x20;
        fs::write(&stl, changed).unwrap();
        assert!(doc.check_stl(&stl).is_err());
        fs::remove_file(&stl).unwrap();
        assert!(doc.check_stl(&stl).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_json_version_and_geometry_fail_without_overwrite() {
        let dir = workspace();
        let file = dir.join("p.lagucam");
        fs::write(&file, b"{broken").unwrap();
        assert!(read_project(&file).is_err());
        let stl = dir.join("m.stl");
        let info = test_stl(&stl);
        let project = configured(info);
        let mut doc = ProjectDocument::from_project(&project, Some(&stl), &file).unwrap();
        doc.version += 1;
        let incompatible = serde_json::to_vec(&doc).unwrap();
        fs::write(&file, &incompatible).unwrap();
        assert!(read_project(&file).is_err());
        fs::write(&file, b"{broken").unwrap();
        assert!(write_project(&file, &doc).is_err());
        doc.version = FORMAT_VERSION;
        doc.state.stock.as_mut().unwrap().size[1] = -2.0;
        assert!(write_project(&file, &doc).is_err());
        doc.state.stock.as_mut().unwrap().size[1] = 6.75;
        doc.state.origin.point = SavedPoint::Corner(11);
        assert!(write_project(&file, &doc).is_err());
        doc.state.origin.point = SavedPoint::Corner(7);
        doc.state.origin.offset[0] = f64::NAN;
        assert!(write_project(&file, &doc).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"{broken");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_replacement_preserves_existing_document() {
        let dir = workspace();
        let file = dir.join("p.lagucam");
        let doc = ProjectDocument::from_project(&Project::default(), None, &file).unwrap();
        write_project(&file, &doc).unwrap();
        let before = fs::read(&file).unwrap();
        // Sin permisos de escribir en carpeta o destino inválido debe conservar el anterior.
        let invalid = dir.join("no_existe").join("p.lagucam");
        assert!(write_project(&invalid, &doc).is_err());
        let mut replacement = doc.clone();
        replacement.state.model_color = [1, 2, 3];
        assert!(
            write_project_with(&file, &replacement, |_, _, _| {
                Err("Fallo simulado durante la sustitución".into())
            })
            .is_err()
        );
        assert_eq!(fs::read(&file).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn mismatched_metadata_never_changes_previous_project_state() {
        let dir = workspace();
        let path = dir.join("m.stl");
        let info = test_stl(&path);
        let doc =
            ProjectDocument::from_project(&configured(info), Some(&path), &dir.join("p.lagucam"))
                .unwrap();
        let previous = ProjectState::capture(&configured(info));
        let mismatched = StlInfo {
            triangles: 2,
            ..info
        };
        assert!(doc.check_loaded(mismatched).is_err());
        assert_eq!(ProjectState::capture(&configured(info)), previous);
        fs::remove_dir_all(dir).unwrap();
    }
}

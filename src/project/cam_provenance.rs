//! P1-A: procedencia e invalidacion de los resultados del acabado paralelo TEST.
//!
//! No almacena trayectorias en .lagucam ni certifica colisiones. Se conserva
//! la hipotesis actual: una unidad STL equivale a un milimetro.

use super::{
    Project,
    operations::{OperationKind, ParallelTestParameters, TestAreaXY},
    persistence::sample_identity,
};
use crate::tools::{
    cam_adapter::{ToolSignature, signature_current},
    model::ToolLibrary,
};
use cam_libre_visor::toolpath::RasterPattern;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

// Cambiar al modificar la semantica del muestreo o la planificacion de enlaces.
const CALCULATION_VERSION: u32 = 1;

/// Parametros que determinan el contacto y los movimientos en coordenadas modelo.
/// No incluye F/S (tecnologia), nombre ni el diametro legado: la herramienta
/// efectiva procede exclusivamente del UUID resuelto en la biblioteca.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalculationInputs {
    stepover: f64,
    longitudinal: f64,
    along_x: bool,
    allowance_mm: f64,
    pass_pattern: RasterPattern,
    clearance_mm: f64,
    approach_mm: f64,
    area_xy: Option<TestAreaXY>,
}

impl From<ParallelTestParameters> for CalculationInputs {
    fn from(p: ParallelTestParameters) -> Self {
        Self {
            stepover: p.stepover,
            longitudinal: p.longitudinal,
            along_x: p.along_x,
            allowance_mm: p.allowance_mm,
            pass_pattern: p.pass_pattern,
            clearance_mm: p.clearance_mm,
            approach_mm: p.approach_mm,
            area_xy: p.area_xy,
        }
    }
}

/// Instantanea inmutable de los datos realmente usados por un calculo.
/// El origen CNC no pertenece aqui: F1.2 lo reproyecta sin alterar contactos.
#[derive(Clone, Debug)]
pub struct CamProvenance {
    source_path: PathBuf,
    source_len: u64,
    source_triangles: u32,
    source_sample_fnv64: String,
    source_modified: SystemTime,
    model_bounds_min: [f64; 3],
    model_bounds_max: [f64; 3],
    model_transform: super::model_transform::ModelTransform,
    stock_min: [f64; 3],
    stock_size: [f64; 3],
    tool: ToolSignature,
    calculation: CalculationInputs,
    version: u32,
}

impl CamProvenance {
    pub fn capture(
        project: &Project,
        operation_index: usize,
        source_path: &Path,
        tool: ToolSignature,
    ) -> Result<Self, String> {
        let model = project.model.as_ref().ok_or("Falta el STL del proyecto")?;
        let stock = project.stock.as_ref().ok_or("Falta configurar el bruto")?;
        stock.validate().map_err(str::to_owned)?;
        let operation = project
            .operations
            .items
            .get(operation_index)
            .ok_or("No existe la operacion CAM")?;
        let OperationKind::ParallelTest(params) = &operation.kind else {
            return Err("La operacion no es acabado paralelo TEST".into());
        };
        (*params).validate()?;
        if operation.tool_id.as_deref() != Some(tool.tool_id.as_str()) {
            return Err("La herramienta CAM no coincide con la operacion".into());
        }
        let path = source_path
            .canonicalize()
            .map_err(|e| format!("No se puede resolver el STL para CAM: {e}"))?;
        let metadata =
            fs::metadata(&path).map_err(|e| format!("No se pueden leer metadatos STL: {e}"))?;
        if !metadata.is_file() {
            return Err("La referencia STL no es un archivo".into());
        }
        let modified = metadata
            .modified()
            .map_err(|e| format!("No se puede verificar la fecha del STL: {e}"))?;
        let (source_len, source_triangles, signature, _) = sample_identity(&path)?;
        if source_len != model.size_bytes || source_triangles != model.triangles {
            return Err("El STL ha cambiado desde que se importo".into());
        }
        Ok(Self {
            source_path: path,
            source_len,
            source_triangles,
            source_sample_fnv64: signature,
            source_modified: modified,
            model_bounds_min: model.bounds.min,
            model_bounds_max: model.bounds.max,
            model_transform: project.model_transform,
            stock_min: stock.min,
            stock_size: stock.size,
            tool,
            calculation: (*params).into(),
            version: CALCULATION_VERSION,
        })
    }

    /// Comprobacion barata para cada ciclo UI: no recorre ni relee el STL.
    /// Una modificacion de archivo que falsee tambien fecha y tamano puede
    /// escapar a esta comprobacion; la firma muestreada se revisa al terminar.
    pub fn is_current(
        &self,
        project: &Project,
        operation_index: usize,
        library: &ToolLibrary,
    ) -> bool {
        let Some(model) = &project.model else {
            return false;
        };
        let Some(stock) = &project.stock else {
            return false;
        };
        let Some(op) = project.operations.items.get(operation_index) else {
            return false;
        };
        let OperationKind::ParallelTest(params) = &op.kind else {
            return false;
        };
        if self.version != CALCULATION_VERSION
            || model.size_bytes != self.source_len
            || model.triangles != self.source_triangles
            || model.bounds.min != self.model_bounds_min
            || model.bounds.max != self.model_bounds_max
            || project.model_transform != self.model_transform
            || stock.min != self.stock_min
            || stock.size != self.stock_size
            || self.calculation != (*params).into()
            || !signature_current(library, op.tool_id.as_deref(), &self.tool)
        {
            return false;
        }
        let Ok(metadata) = fs::metadata(&self.source_path) else {
            return false;
        };
        metadata.is_file()
            && metadata.len() == self.source_len
            && metadata
                .modified()
                .is_ok_and(|date| date == self.source_modified)
    }

    /// Revision de la firma muestreada del STL antes de aceptar el resultado.
    /// No es un hash completo ni una comprobacion anticolisiones.
    pub fn source_samples_current(&self) -> bool {
        sample_identity(&self.source_path).is_ok_and(|(len, triangles, signature, _)| {
            len == self.source_len
                && triangles == self.source_triangles
                && signature == self.source_sample_fnv64
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        project::{
            model::ModelReference, operations::OperationKind, origin::WorkOrigin, stock::Stock,
        },
        stl::Bounds,
        tools::{
            cam_adapter::resolve_cutter,
            model::{Tool, ToolGeometry, ToolKind},
        },
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST: AtomicUsize = AtomicUsize::new(0);

    fn sample_fixture() -> (Project, ToolLibrary, PathBuf, CamProvenance) {
        let path = std::env::temp_dir().join(format!(
            "lagucam_p1a_{}_{}.stl",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        let mut stl = vec![0u8; 134];
        stl[80..84].copy_from_slice(&1u32.to_le_bytes());
        fs::write(&path, stl).unwrap();
        let bounds = Bounds {
            min: [0.0; 3],
            max: [3.0; 3],
        };
        let mut project = Project::default();
        project.model = Some(ModelReference {
            name: "patron.stl".into(),
            bounds,
            raw_bounds: bounds,
            triangles: 1,
            size_bytes: 134,
        });
        project.stock = Some(Stock::from_bounds(&bounds));
        let index = project.operations.add_parallel_test().unwrap();
        let mut library = ToolLibrary::default();
        let mut cutter = Tool::blank(ToolKind::BallNose);
        cutter.name = "Fresa esferica P1-A".into();
        cutter.geometry = ToolGeometry::BallNose {
            cutting_diameter_mm: 4.0,
        };
        cutter.cutting_length_mm = 15.0;
        cutter.shank_diameter_mm = 4.0;
        cutter.total_length_mm = 30.0;
        let id = library.add(cutter).unwrap();
        project.operations.items[index].tool_id = Some(id.clone());
        let signature = resolve_cutter(&library, Some(&id), 1.0).unwrap().signature;
        let provenance = CamProvenance::capture(&project, index, &path, signature).unwrap();
        (project, library, path, provenance)
    }

    #[test]
    fn stock_strategy_and_tool_changes_expire_results() {
        let (mut project, mut library, path, stamp) = sample_fixture();
        assert!(stamp.is_current(&project, 0, &library));
        project.stock.as_mut().unwrap().min[2] += 1.0;
        assert!(!stamp.is_current(&project, 0, &library));
        project.stock.as_mut().unwrap().min[2] -= 1.0;
        if let OperationKind::ParallelTest(ref mut p) = project.operations.items[0].kind {
            p.stepover += 1.0;
        }
        assert!(!stamp.is_current(&project, 0, &library));
        if let OperationKind::ParallelTest(ref mut p) = project.operations.items[0].kind {
            p.stepover -= 1.0;
        }
        let id = project.operations.items[0].tool_id.clone().unwrap();
        let mut tool = library.get(&id).unwrap().clone();
        tool.geometry = ToolGeometry::BallNose {
            cutting_diameter_mm: 5.0,
        };
        library.replace(tool).unwrap();
        assert!(!stamp.is_current(&project, 0, &library));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn display_origin_feeds_and_legacy_diameter_do_not_change_contacts() {
        let (mut project, library, path, stamp) = sample_fixture();
        project.stock.as_mut().unwrap().visible = false;
        project.origin = WorkOrigin {
            offset: [2.0, 0.0, 0.0],
            ..WorkOrigin::default()
        };
        if let OperationKind::ParallelTest(ref mut p) = project.operations.items[0].kind {
            p.diameter += 1.0;
            p.cutting_feed_mm_min = Some(1200.0);
            p.spindle_rpm = Some(16000.0);
        }
        assert!(stamp.is_current(&project, 0, &library));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn changing_transform_always_invalidates_even_with_equal_aabb() {
        let (mut project, library, path, stamp) = sample_fixture();
        assert!(stamp.is_current(&project, 0, &library));
        project.model_transform.rotation_degrees[2] = 180.0;
        assert!(!stamp.is_current(&project, 0, &library));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn source_change_or_missing_operation_is_not_current() {
        let (mut project, library, path, stamp) = sample_fixture();
        assert!(stamp.source_samples_current());
        let mut modified = fs::read(&path).unwrap();
        modified.push(0);
        fs::write(&path, modified).unwrap();
        assert!(!stamp.source_samples_current());
        assert!(!stamp.is_current(&project, 0, &library));
        project.operations.items.clear();
        assert!(!stamp.is_current(&project, 0, &library));
        fs::remove_file(path).unwrap();
    }
}

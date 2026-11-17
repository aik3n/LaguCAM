//! Fachada experimental G17 para consultas de contacto de fresa esférica.
//!
//! Contrato de coordenadas: `x_model`, `y_model`, `radius_model` y `tip_z_model`
//! usan **las mismas unidades y el mismo marco** que las coordenadas del STL
//! original. STL no declara unidades: aquí NO se aplica el origen CNC, la
//! orientación del setup, ni el escalado. Es responsabilidad del llamante.
//!
//! Un resultado `Some(z)` es la cota geométrica de la punta inferior de una
//! ESFERA, no una trayectoria ni un certificado de ausencia de colisiones.
//! `None` significa que no se encontró contacto en el radio consultado; no
//! equivale a una autorización de movimiento ni a una altura segura.
//!
//! La protección de archivo de esta fachada está disponible en Windows.
//! En otros sistemas se devuelve `UnsupportedPlatform` hasta establecer una
//! política de inmutabilidad de archivo adecuada.

use super::ContactError;
#[cfg(windows)]
use super::stl::BuildControl;
use super::stl::{
    BuildStats, FileBvh, G16AccessMode, G16MemoryEstimate, GeometryAffine, StlBvhError,
};
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// El presupuesto es de estructuras contabilizadas del motor, no RSS total.
#[derive(Clone, Copy, Debug)]
pub struct ContactEngineLimits {
    pub max_triangles: usize,
    pub max_accounted_bytes: usize,
    pub cache_bytes: usize,
}

/// Posición XY y radio de la esfera en coordenadas/ unidades del STL original.
#[derive(Clone, Copy, Debug)]
pub struct BallContactQuery {
    pub x_model: f64,
    pub y_model: f64,
    pub radius_model: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ContactEngineOpenReport {
    pub triangles: usize,
    pub nodes: usize,
    pub build: BuildStats,
    pub memory: G16MemoryEstimate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallContactResult {
    /// Altura de la punta inferior en el sistema de coordenadas original del STL.
    pub tip_z_model: Option<f64>,
    pub triangles_considered: usize,
    pub nodes_visited: usize,
    pub physical_block_reads: usize,
    pub mode: G16AccessMode,
}

/// Errores tipados que la capa de operaciones podrá distinguir sin analizar cadenas.
#[derive(Debug)]
pub enum ContactEngineError {
    Cancelled,
    InvalidQuery,
    InvalidStlOrLimit(&'static str),
    SourceChanged,
    ArithmeticRange,
    Io(std::io::Error),
    UnsupportedPlatform,
}

impl fmt::Display for ContactEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Consulta o construcción cancelada"),
            Self::InvalidQuery => f.write_str("Parámetros de consulta inválidos"),
            Self::InvalidStlOrLimit(msg) => write!(f, "STL o límites inválidos: {msg}"),
            Self::SourceChanged => f.write_str("El STL ha cambiado durante su uso"),
            Self::ArithmeticRange => f.write_str("Fuera del rango aritmético seguro"),
            Self::Io(error) => write!(f, "Error de lectura STL: {error}"),
            Self::UnsupportedPlatform => {
                f.write_str("Protección de STL no implementada en esta plataforma")
            }
        }
    }
}

impl std::error::Error for ContactEngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StlBvhError> for ContactEngineError {
    fn from(error: StlBvhError) -> Self {
        match error {
            StlBvhError::Cancelled => Self::Cancelled,
            StlBvhError::Io(error) => Self::Io(error),
            StlBvhError::InvalidStl(msg) => Self::InvalidStlOrLimit(msg),
            StlBvhError::Geometry(ContactError::InvalidInput) => Self::InvalidQuery,
            StlBvhError::Geometry(ContactError::ArithmeticRange) | StlBvhError::ArithmeticRange => {
                Self::ArithmeticRange
            }
            StlBvhError::SourceChanged => Self::SourceChanged,
        }
    }
}

/// Servicio con acceso exclusivo de escritura denegado mientras está abierto
/// (Windows). No es `Clone`, y cada consulta requiere `&mut self`.
pub struct BallContactEngine {
    index: FileBvh,
}

impl BallContactEngine {
    /// Construye el índice y activa la selección automática G16.
    /// El token puede establecerse desde otro hilo durante el trabajo.
    /// La cancelación es cooperativa, no interrumpe una E/S o partición en curso.
    pub fn open_stl(
        path: &Path,
        limits: ContactEngineLimits,
        cancelled: &AtomicBool,
    ) -> Result<(Self, ContactEngineOpenReport), ContactEngineError> {
        Self::open_stl_affine(path, limits, cancelled, None)
    }

    pub fn open_stl_transformed(
        path: &Path,
        limits: ContactEngineLimits,
        cancelled: &AtomicBool,
        affine: GeometryAffine,
    ) -> Result<(Self, ContactEngineOpenReport), ContactEngineError> {
        Self::open_stl_affine(path, limits, cancelled, Some(affine))
    }

    fn open_stl_affine(
        path: &Path,
        limits: ContactEngineLimits,
        cancelled: &AtomicBool,
        affine: Option<GeometryAffine>,
    ) -> Result<(Self, ContactEngineOpenReport), ContactEngineError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactEngineError::Cancelled);
        }
        if limits.max_triangles == 0 || limits.max_accounted_bytes == 0 {
            return Err(ContactEngineError::InvalidStlOrLimit(
                "Límites de construcción nulos",
            ));
        }
        #[cfg(not(windows))]
        {
            let _ = (path, limits, affine);
            return Err(ContactEngineError::UnsupportedPlatform);
        }
        #[cfg(windows)]
        {
            let control = BuildControl {
                max_triangles: limits.max_triangles,
                max_index_bytes: limits.max_accounted_bytes,
                cancel_file: None,
            };
            let (mut index, build) = match affine {
                Some(affine) => {
                    FileBvh::build_g17_cancelable_transformed(path, &control, cancelled, affine)?
                }
                None => FileBvh::build_g17_cancelable(path, &control, cancelled)?,
            };
            if cancelled.load(Ordering::Relaxed) {
                return Err(ContactEngineError::Cancelled);
            }
            let memory =
                index.configure_g16_auto_cache(limits.cache_bytes, limits.max_accounted_bytes)?;
            let report = ContactEngineOpenReport {
                triangles: index.triangle_count(),
                nodes: index.node_count(),
                build,
                memory,
            };
            Ok((Self { index }, report))
        }
    }

    /// Acceso interno para G21. No modifica el contrato publico esferico G18.
    pub(super) fn query_flat(
        &mut self,
        request: BallContactQuery,
        cancelled: &AtomicBool,
    ) -> Result<BallContactResult, ContactEngineError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactEngineError::Cancelled);
        }
        if !request.x_model.is_finite()
            || !request.y_model.is_finite()
            || !request.radius_model.is_finite()
            || request.radius_model <= 0.0
        {
            return Err(ContactEngineError::InvalidQuery);
        }
        let (tip_z_model, stats, io, decision) = self.index.query_g21_flat_cancelable(
            request.x_model,
            request.y_model,
            request.radius_model,
            cancelled,
        )?;
        Ok(BallContactResult {
            tip_z_model,
            triangles_considered: stats.candidates,
            nodes_visited: stats.nodes_visited,
            physical_block_reads: io.block_reads,
            mode: decision.mode,
        })
    }

    /// G24: consulta toroidal nominal o con sobrematerial, reutilizando G16.
    pub(super) fn query_bull(
        &mut self,
        request: BallContactQuery,
        corner_radius_model: f64,
        allowance_model: f64,
        cancelled: &AtomicBool,
    ) -> Result<BallContactResult, ContactEngineError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactEngineError::Cancelled);
        }
        let outer = request.radius_model + allowance_model;
        let corner = corner_radius_model + allowance_model;
        if !request.x_model.is_finite()
            || !request.y_model.is_finite()
            || !request.radius_model.is_finite()
            || request.radius_model <= 0.0
            || !corner_radius_model.is_finite()
            || corner_radius_model <= 0.0
            || corner_radius_model > request.radius_model
            || !allowance_model.is_finite()
            || allowance_model < 0.0
        {
            return Err(ContactEngineError::InvalidQuery);
        }
        if !(outer * outer).is_finite() || !(corner * corner).is_finite() {
            return Err(ContactEngineError::ArithmeticRange);
        }
        let (tip_z_model, stats, io, decision) = self.index.query_g24_bull_cancelable(
            request.x_model,
            request.y_model,
            request.radius_model,
            corner_radius_model,
            allowance_model,
            cancelled,
        )?;
        Ok(BallContactResult {
            tip_z_model,
            triangles_considered: stats.candidates,
            nodes_visited: stats.nodes_visited,
            physical_block_reads: io.block_reads,
            mode: decision.mode,
        })
    }

    /// G22: fresa plana con dilatacion euclidea positiva del STL.
    /// Nunca se llama con a=0, conservando la ruta G21 exactamente.
    pub(super) fn query_flat_allowance(
        &mut self,
        request: BallContactQuery,
        allowance_model: f64,
        cancelled: &AtomicBool,
    ) -> Result<BallContactResult, ContactEngineError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactEngineError::Cancelled);
        }
        if !request.x_model.is_finite()
            || !request.y_model.is_finite()
            || !request.radius_model.is_finite()
            || request.radius_model <= 0.0
            || !allowance_model.is_finite()
            || allowance_model <= 0.0
        {
            return Err(ContactEngineError::InvalidQuery);
        }
        let (tip_z_model, stats, io, decision) = self.index.query_g22_flat_allowance_cancelable(
            request.x_model,
            request.y_model,
            request.radius_model,
            allowance_model,
            cancelled,
        )?;
        Ok(BallContactResult {
            tip_z_model,
            triangles_considered: stats.candidates,
            nodes_visited: stats.nodes_visited,
            physical_block_reads: io.block_reads,
            mode: decision.mode,
        })
    }

    /// Consulta de solo lectura del STL protegido. Un error o cancelación
    /// nunca devuelve una altura parcial. El resultado no verifica colisiones.
    pub fn query(
        &mut self,
        request: BallContactQuery,
        cancelled: &AtomicBool,
    ) -> Result<BallContactResult, ContactEngineError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(ContactEngineError::Cancelled);
        }
        if !request.x_model.is_finite()
            || !request.y_model.is_finite()
            || !request.radius_model.is_finite()
            || request.radius_model <= 0.0
        {
            return Err(ContactEngineError::InvalidQuery);
        }
        let (tip_z_model, stats, io, decision) = self.index.query_g17_auto_cancelable(
            request.x_model,
            request.y_model,
            request.radius_model,
            cancelled,
        )?;
        Ok(BallContactResult {
            tip_z_model,
            triangles_considered: stats.candidates,
            nodes_visited: stats.nodes_visited,
            physical_block_reads: io.block_reads,
            mode: decision.mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ContactEngineLimits {
        ContactEngineLimits {
            max_triangles: 100,
            max_accounted_bytes: 2_000_000,
            cache_bytes: 1024 * 50,
        }
    }

    #[test]
    fn cancelled_open_wins_over_missing_file() {
        let cancelled = AtomicBool::new(true);
        let result =
            BallContactEngine::open_stl(Path::new("missing_stl_g17.stl"), limits(), &cancelled);
        assert!(matches!(result, Err(ContactEngineError::Cancelled)));
    }

    #[test]
    fn zero_budget_is_rejected_before_read() {
        let cancelled = AtomicBool::new(false);
        let mut bad = limits();
        bad.max_accounted_bytes = 0;
        let result = BallContactEngine::open_stl(Path::new("missing_stl_g17.stl"), bad, &cancelled);
        assert!(matches!(
            result,
            Err(ContactEngineError::InvalidStlOrLimit(_))
        ));
    }

    #[test]
    fn low_level_errors_remain_distinguishable() {
        let cancelled: ContactEngineError = StlBvhError::Cancelled.into();
        assert!(matches!(cancelled, ContactEngineError::Cancelled));
        let changed: ContactEngineError = StlBvhError::SourceChanged.into();
        assert!(matches!(changed, ContactEngineError::SourceChanged));
        let invalid: ContactEngineError = StlBvhError::Geometry(ContactError::InvalidInput).into();
        assert!(matches!(invalid, ContactEngineError::InvalidQuery));
        let numeric: ContactEngineError = StlBvhError::ArithmeticRange.into();
        assert!(matches!(numeric, ContactEngineError::ArithmeticRange));
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_is_explicit() {
        let cancelled = AtomicBool::new(false);
        assert!(matches!(
            BallContactEngine::open_stl(Path::new("missing_stl_g17.stl"), limits(), &cancelled),
            Err(ContactEngineError::UnsupportedPlatform)
        ));
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;
        use std::sync::atomic::AtomicU64;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct TemporaryStl(std::path::PathBuf);
        impl Drop for TemporaryStl {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        fn triangle_stl() -> TemporaryStl {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir()
                    .join(format!("lagucam_g17_{}_{}.stl", std::process::id(), id,));
            let mut bytes = vec![0_u8; 80];
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.extend_from_slice(&[0_u8; 12]);
            for v in [[0.0_f32, 0.0, 5.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0]] {
                for axis in v {
                    bytes.extend_from_slice(&axis.to_le_bytes());
                }
            }
            bytes.extend_from_slice(&[0_u8; 2]);
            std::fs::write(&path, bytes).unwrap();
            TemporaryStl(path)
        }
        #[test]
        fn public_api_contact_none_invalid_and_cancelled_query() {
            let file = triangle_stl();
            let cancel = AtomicBool::new(false);
            let (mut engine, report) =
                BallContactEngine::open_stl(&file.0, limits(), &cancel).unwrap();
            assert_eq!(report.triangles, 1);
            let at_face = BallContactQuery {
                x_model: 2.0,
                y_model: 2.0,
                radius_model: 1.0,
            };
            assert_eq!(
                engine.query(at_face, &cancel).unwrap().tip_z_model,
                Some(5.0)
            );
            let outside = BallContactQuery {
                x_model: 100.0,
                y_model: 100.0,
                radius_model: 1.0,
            };
            assert_eq!(engine.query(outside, &cancel).unwrap().tip_z_model, None);
            let invalid = BallContactQuery {
                radius_model: 0.0,
                ..at_face
            };
            assert!(matches!(
                engine.query(invalid, &cancel),
                Err(ContactEngineError::InvalidQuery)
            ));
            cancel.store(true, Ordering::Relaxed);
            assert!(matches!(
                engine.query(at_face, &cancel),
                Err(ContactEngineError::Cancelled)
            ));
            cancel.store(false, Ordering::Relaxed);
            assert_eq!(
                engine.query(at_face, &cancel).unwrap().tip_z_model,
                Some(5.0)
            );
        }
    }
}

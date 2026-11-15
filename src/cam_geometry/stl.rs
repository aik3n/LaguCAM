//! Adaptador STL binario con índices de registros y BVH compacta en XY.
//! El STL no se escribe ni se guarda una copia completa de sus triángulos.
//! En Windows, el archivo permanece abierto sin compartir escritura/borrado.

use super::flat_allowance::flat_tip_height_triangle_allowance;
use super::{ContactError, Vec3, ball_tip_height_triangle, flat_tip_height_triangle};
use std::collections::HashMap;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

const BVH_LEAF_SIZE: usize = 8;
const G15_RECORDS_PER_BLOCK: usize = 1024;
const G15_BLOCK_BYTES: usize = G15_RECORDS_PER_BLOCK * 50;
const G15_MAX_CACHE_BYTES: usize = 32 * 1024 * 1024;

fn open_read_protected(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ evita nuevas aperturas incompatibles para escritura/borrado.
        options.share_mode(0x0000_0001);
    }
    // En otros sistemas, la protección contra escrituras concurrentes
    // requiere un mecanismo adicional antes de integrar con operaciones CAM.
    options.open(path)
}

// Índice BVH compacto: cajas XY y números de registro, sin malla duplicada.

#[derive(Debug)]
pub enum StlBvhError {
    Io(std::io::Error),
    InvalidStl(&'static str),
    Geometry(ContactError),
    SourceChanged,
    ArithmeticRange,
    /// Cancelación cooperativa solicitada por el llamante.
    Cancelled,
}
impl From<std::io::Error> for StlBvhError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<ContactError> for StlBvhError {
    fn from(error: ContactError) -> Self {
        Self::Geometry(error)
    }
}

/// El token de G17 se comprueba durante la construcción y cada lote de consulta.
/// No puede interrumpir una operación individual de E/S ni una partición std.
fn g17_check_cancel(cancel: Option<&AtomicBool>) -> Result<(), StlBvhError> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        Err(StlBvhError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct PrimitiveRecord {
    // F32 corresponde exactamente a coordenadas del STL original.
    xmin: f32,
    xmax: f32,
    ymin: f32,
    ymax: f32,
    id: u32,
}
impl PrimitiveRecord {
    fn from_tri(t: &[[f32; 3]; 3], id: u32) -> Self {
        let (mut xmin, mut xmax) = (t[0][0], t[0][0]);
        let (mut ymin, mut ymax) = (t[0][1], t[0][1]);
        for vertex in &t[1..] {
            xmin = xmin.min(vertex[0]);
            xmax = xmax.max(vertex[0]);
            ymin = ymin.min(vertex[1]);
            ymax = ymax.max(vertex[1]);
        }
        Self {
            xmin,
            xmax,
            ymin,
            ymax,
            id,
        }
    }
    fn centroid(self, axis: usize) -> f64 {
        if axis == 0 {
            (f64::from(self.xmin) + f64::from(self.xmax)) * 0.5
        } else {
            (f64::from(self.ymin) + f64::from(self.ymax)) * 0.5
        }
    }
    fn bounds(self) -> [f32; 4] {
        [self.xmin, self.xmax, self.ymin, self.ymax]
    }
}

fn union_f32_bounds(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].max(b[3]),
    ]
}
fn disk_could_intersect(bounds: [f32; 4], x: f64, y: f64, radius: f64) -> bool {
    // Comparamos en f64 con los f32 originales convertidos exactamente.
    // Restas de magnitudes extremas se permiten: inf no crea falsos descartes
    // salvo distancias verdaderamente fuera del radio finito.
    let xmin = f64::from(bounds[0]);
    let xmax = f64::from(bounds[1]);
    let ymin = f64::from(bounds[2]);
    let ymax = f64::from(bounds[3]);
    let dx = if x < xmin {
        xmin - x
    } else if x > xmax {
        x - xmax
    } else {
        0.0
    };
    let dy = if y < ymin {
        ymin - y
    } else if y > ymax {
        y - ymax
    } else {
        0.0
    };
    // Conservador para los contactos exactamente sobre el borde.
    dx <= radius.next_up() && dy <= radius.next_up()
}

const NO_CHILD: u32 = u32::MAX;
#[derive(Clone, Copy, Debug)]
struct FileBvhNode {
    bounds: [f32; 4],
    left: u32,
    right: u32,
    start: u32,
    end: u32,
}
impl FileBvhNode {
    fn leaf(bounds: [f32; 4], start: usize, end: usize) -> Self {
        Self {
            bounds,
            left: NO_CHILD,
            right: NO_CHILD,
            start: start as u32,
            end: end as u32,
        }
    }
}

fn stl_triangle_f32(record: &[u8; 50]) -> Result<[[f32; 3]; 3], StlBvhError> {
    let mut t = [[0.0_f32; 3]; 3];
    for (v, vertex) in t.iter_mut().enumerate() {
        for (axis, coord) in vertex.iter_mut().enumerate() {
            let offset = 12 + v * 12 + axis * 4;
            *coord = f32::from_le_bytes(record[offset..offset + 4].try_into().unwrap());
            if !coord.is_finite() {
                return Err(StlBvhError::InvalidStl("Coordenadas no finitas"));
            }
        }
    }
    Ok(t)
}
fn stl_triangle_f64(record: &[u8; 50]) -> Result<[Vec3; 3], StlBvhError> {
    let t = stl_triangle_f32(record)?;
    Ok(t.map(|v| Vec3 {
        x: f64::from(v[0]),
        y: f64::from(v[1]),
        z: f64::from(v[2]),
    }))
}

fn source_stamp(metadata: &Metadata) -> (u64, Option<SystemTime>) {
    (metadata.len(), metadata.modified().ok())
}

pub struct FileBvh {
    source: File,
    original_stamp: (u64, Option<SystemTime>),
    records: Vec<PrimitiveRecord>,
    nodes: Vec<FileBvhNode>,
    block_cache: Option<G15BlockCache>,
    g16_budget: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileQueryStats {
    pub nodes_visited: usize,
    pub candidates: usize,
    pub stl_records_read: usize,
}

/// Estadísticas independientes para G15: un acierto es un registro lógico
/// resuelto desde un bloque ya cargado, sin lectura adicional del STL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct G15IoStats {
    pub block_reads: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub bytes_read: usize,
}

struct G15CacheEntry {
    data: Vec<u8>,
    last_access: u64,
}

/// Caché LRU asociada al *mismo* FileBvh: no se puede reutilizar con otro STL.
struct G15BlockCache {
    blocks: HashMap<u32, G15CacheEntry>,
    capacity_blocks: usize,
    counter: u64,
}

impl G15BlockCache {
    fn new(max_bytes: usize) -> Result<Self, StlBvhError> {
        if !(G15_BLOCK_BYTES..=G15_MAX_CACHE_BYTES).contains(&max_bytes) {
            return Err(StlBvhError::InvalidStl("Capacidad G15 fuera de rango"));
        }
        Ok(Self {
            blocks: HashMap::new(),
            capacity_blocks: max_bytes / G15_BLOCK_BYTES,
            counter: 0,
        })
    }

    fn capacity_bytes(&self) -> usize {
        self.capacity_blocks * G15_BLOCK_BYTES
    }

    fn record<'a>(
        &'a mut self,
        source: &mut File,
        total_records: usize,
        id: u32,
        stats: &mut G15IoStats,
    ) -> Result<&'a [u8; 50], StlBvhError> {
        let id = id as usize;
        if id >= total_records {
            return Err(StlBvhError::InvalidStl("ID de triángulo fuera del STL"));
        }
        let block_id = (id / G15_RECORDS_PER_BLOCK) as u32;
        // El contador solo se utiliza para escoger la entrada menos reciente.
        // El reinicio evita que un uso extraordinariamente largo lo desborde.
        if self.counter == u64::MAX {
            for entry in self.blocks.values_mut() {
                entry.last_access = 0;
            }
            self.counter = 0;
        }
        self.counter += 1;
        if !self.blocks.contains_key(&block_id) {
            stats.cache_misses += 1;
            let first_id = (block_id as usize) * G15_RECORDS_PER_BLOCK;
            let record_count = (total_records - first_id).min(G15_RECORDS_PER_BLOCK);
            let read_bytes = record_count * 50;
            // Reutilizar el bloque expulsado reduce asignaciones y no cambia
            // el orden en que se evalúan los contactos de la BVH.
            let mut entry = if self.blocks.len() == self.capacity_blocks {
                let oldest = self
                    .blocks
                    .iter()
                    .min_by_key(|(_, v)| v.last_access)
                    .map(|(&key, _)| key)
                    .ok_or(StlBvhError::ArithmeticRange)?;
                self.blocks.remove(&oldest).unwrap()
            } else {
                G15CacheEntry {
                    data: Vec::new(),
                    last_access: 0,
                }
            };
            entry.data.resize(read_bytes, 0);
            source.seek(SeekFrom::Start(84 + (first_id as u64) * 50))?;
            source.read_exact(&mut entry.data)?;
            stats.block_reads += 1;
            stats.bytes_read += read_bytes;
            entry.last_access = self.counter;
            self.blocks.insert(block_id, entry);
        } else {
            stats.cache_hits += 1;
        }
        let entry = self.blocks.get_mut(&block_id).unwrap();
        entry.last_access = self.counter;
        let offset = (id % G15_RECORDS_PER_BLOCK) * 50;
        let slice = entry
            .data
            .get(offset..offset + 50)
            .ok_or(StlBvhError::InvalidStl("Bloque STL incompleto"))?;
        Ok(slice.try_into().unwrap())
    }
}

/// Política experimental G16, aplicada después de un muestreo de candidatos
/// en la BVH. No decide basándose exclusivamente en el radio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum G16AccessMode {
    Blocks,
    Cached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct G16Decision {
    pub mode: G16AccessMode,
    pub sampled_candidates: usize,
    pub sampled_cached_records: usize,
}

/// Presupuesto conservador de las asignaciones controladas por el módulo.
/// No es una cota de RSS del proceso ni cuenta las cachés del SO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct G16MemoryEstimate {
    pub index_capacity_bytes: usize,
    pub cache_payload_limit_bytes: usize,
    pub cache_table_allowance_bytes: usize,
    pub temporary_allowance_bytes: usize,
    pub accounted_bytes: usize,
    pub configured_budget_bytes: usize,
}

const G16_AUTO_LARGE_SAMPLE: usize = 16_384;
const G16_AUTO_WARM_MIN_SAMPLE: usize = 1_024;
// Reserva holgada para vectores de hasta 4096 candidatos, una ventana
// de bloque temporal y pequeñas estructuras de consulta. No cuenta
// memoria ajena al motor ni asignaciones internas del sistema operativo.
const G16_TEMPORARY_ALLOWANCE: usize = 1_048_576;

fn g16_table_allowance(capacity_blocks: usize) -> Result<usize, StlBvhError> {
    // std::HashMap no expone su consumo exacto. Se reserva una estimación
    // conservadora de hasta el doble de la siguiente potencia de dos
    // para buckets y bytes de control. No equivale a un límite contractual
    // del asignador del proceso.
    let buckets = capacity_blocks
        .max(1)
        .checked_next_power_of_two()
        .and_then(|v| v.checked_mul(2))
        .ok_or(StlBvhError::ArithmeticRange)?;
    let entry = std::mem::size_of::<(u32, G15CacheEntry)>();
    buckets
        .checked_mul(entry.checked_add(2).ok_or(StlBvhError::ArithmeticRange)?)
        .and_then(|v| v.checked_add(4096))
        .ok_or(StlBvhError::ArithmeticRange)
}

fn g16_choose_mode(sampled: usize, warm: usize) -> G16AccessMode {
    // En G15 los bloques fueron mejores con pocos candidatos fríos;
    // la LRU fue más rápida con >160k candidatos. Los umbrales son
    // provisionales y se medirán con G16 sobre el STL real.
    if sampled >= G16_AUTO_LARGE_SAMPLE
        || (sampled >= G16_AUTO_WARM_MIN_SAMPLE
            && warm.saturating_mul(4) >= sampled.saturating_mul(3))
    {
        G16AccessMode::Cached
    } else {
        G16AccessMode::Blocks
    }
}

impl FileBvh {
    /// Configura la política automática solo si cabe un presupuesto
    /// conservador: índices (capacity real) + payload máximo de caché +
    /// reserva de tabla HashMap + área temporal. El presupuesto no limita
    /// la memoria global del proceso.
    pub fn configure_g16_auto_cache(
        &mut self,
        max_cache_bytes: usize,
        max_accounted_bytes: usize,
    ) -> Result<G16MemoryEstimate, StlBvhError> {
        let cache = G15BlockCache::new(max_cache_bytes)?;
        let payload = cache.capacity_bytes();
        let table = g16_table_allowance(cache.capacity_blocks)?;
        let index = self.allocated_bytes();
        let accounted = index
            .checked_add(payload)
            .and_then(|v| v.checked_add(table))
            .and_then(|v| v.checked_add(G16_TEMPORARY_ALLOWANCE))
            .ok_or(StlBvhError::ArithmeticRange)?;
        if accounted > max_accounted_bytes {
            return Err(StlBvhError::InvalidStl(
                "G16 supera el presupuesto contabilizado",
            ));
        }
        let report = G16MemoryEstimate {
            index_capacity_bytes: index,
            cache_payload_limit_bytes: payload,
            cache_table_allowance_bytes: table,
            temporary_allowance_bytes: G16_TEMPORARY_ALLOWANCE,
            accounted_bytes: accounted,
            configured_budget_bytes: max_accounted_bytes,
        };
        self.block_cache = Some(cache);
        self.g16_budget = Some(max_accounted_bytes);
        Ok(report)
    }

    pub fn g16_memory_estimate(&self) -> Option<G16MemoryEstimate> {
        let budget = self.g16_budget?;
        let cache = self.block_cache.as_ref()?;
        let payload = cache.capacity_bytes();
        let table = g16_table_allowance(cache.capacity_blocks).ok()?;
        let index = self.allocated_bytes();
        let accounted = index
            .checked_add(payload)?
            .checked_add(table)?
            .checked_add(G16_TEMPORARY_ALLOWANCE)?;
        Some(G16MemoryEstimate {
            index_capacity_bytes: index,
            cache_payload_limit_bytes: payload,
            cache_table_allowance_bytes: table,
            temporary_allowance_bytes: G16_TEMPORARY_ALLOWANCE,
            accounted_bytes: accounted,
            configured_budget_bytes: budget,
        })
    }

    fn g16_sample_candidates(
        &self,
        x: f64,
        y: f64,
        radius: f64,
        cache: &G15BlockCache,
    ) -> (usize, usize) {
        let mut count = 0usize;
        let mut warm = 0usize;
        let mut stack = vec![0_u32];
        while let Some(node_id) = stack.pop() {
            let node = self.nodes[node_id as usize];
            if !disk_could_intersect(node.bounds, x, y, radius) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, radius) {
                        continue;
                    }
                    count += 1;
                    let block_id = p.id / (G15_RECORDS_PER_BLOCK as u32);
                    if cache.blocks.contains_key(&block_id) {
                        warm += 1;
                    }
                    if count >= G16_AUTO_LARGE_SAMPLE {
                        return (count, warm);
                    }
                }
            }
        }
        (count, warm)
    }

    /// Selección por densidad local de candidatos y bloques ya presentes.
    /// Requiere configure_g16_auto_cache para exigir el presupuesto.
    /// Las alturas y las estadísticas BVH mantienen equivalencia con G15.
    pub fn query_g16_auto(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats, G16Decision), StlBvhError> {
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        self.check_source()?;
        let cache = self
            .block_cache
            .as_ref()
            .filter(|_| self.g16_budget.is_some())
            .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
        let (count, warm) = self.g16_sample_candidates(x, y, radius, cache);
        let decision = G16Decision {
            mode: g16_choose_mode(count, warm),
            sampled_candidates: count,
            sampled_cached_records: warm,
        };
        let (z, stats, io) = match decision.mode {
            G16AccessMode::Blocks => self.query_g15_blocks(x, y, radius)?,
            G16AccessMode::Cached => self.query_g15_cached(x, y, radius)?,
        };
        Ok((z, stats, io, decision))
    }

    /// Versión con cancelación cooperativa del selector G16. Se comprueba
    /// el token antes y después del muestreo y periódicamente durante la
    /// lectura de candidatos; nunca devuelve una altura parcial si cancela.
    pub fn query_g17_auto_cancelable(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cancel: &AtomicBool,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats, G16Decision), StlBvhError> {
        g17_check_cancel(Some(cancel))?;
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        self.check_source()?;
        let cache = self
            .block_cache
            .as_ref()
            .filter(|_| self.g16_budget.is_some())
            .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
        let (count, warm) = self.g16_sample_candidates(x, y, radius, cache);
        g17_check_cancel(Some(cancel))?;
        let decision = G16Decision {
            mode: g16_choose_mode(count, warm),
            sampled_candidates: count,
            sampled_cached_records: warm,
        };
        let (z, stats, io) = match decision.mode {
            G16AccessMode::Blocks => self.query_g15_blocks_impl(x, y, radius, Some(cancel))?,
            G16AccessMode::Cached => self.query_g15_cached_impl(x, y, radius, Some(cancel))?,
        };
        g17_check_cancel(Some(cancel))?;
        Ok((z, stats, io, decision))
    }
}

impl FileBvh {
    /// Activa una caché de hasta 32 MiB. El presupuesto combinado incluye
    /// el índice BVH y la capacidad nominal de los bloques, no las pequeñas
    /// estructuras de control del HashMap ni memoria ajena al motor.
    pub fn configure_g15_cache(
        &mut self,
        max_cache_bytes: usize,
        max_index_plus_cache_bytes: usize,
    ) -> Result<(), StlBvhError> {
        let cache = G15BlockCache::new(max_cache_bytes)?;
        let total = self
            .allocated_bytes()
            .checked_add(cache.capacity_bytes())
            .ok_or(StlBvhError::ArithmeticRange)?;
        if total > max_index_plus_cache_bytes {
            return Err(StlBvhError::InvalidStl(
                "Índice y caché exceden presupuesto G15",
            ));
        }
        self.block_cache = Some(cache);
        self.g16_budget = None;
        Ok(())
    }

    pub fn g15_cache_capacity_bytes(&self) -> usize {
        self.block_cache
            .as_ref()
            .map_or(0, G15BlockCache::capacity_bytes)
    }

    /// Cada consulta crea una única ventana temporal; sirve como comparación
    /// de lectura agrupada, sin LRU persistente.
    pub fn query_g15_blocks(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        self.query_g15_blocks_impl(x, y, radius, None)
    }

    fn query_g15_blocks_impl(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        g17_check_cancel(cancel)?;
        let mut temporary = G15BlockCache::new(G15_BLOCK_BYTES)?;
        self.query_g15_batched(x, y, radius, &mut temporary, cancel)
    }

    /// Agrupa hasta 4096 candidatos, los lee por ID original y conserva el
    /// orden de reducción de máximos de la BVH. La memoria de trabajo extra
    /// queda acotada independientemente del número total de candidatos.
    fn query_g15_batched(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cache: &mut G15BlockCache,
        cancel: Option<&AtomicBool>,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        g17_check_cancel(cancel)?;
        self.check_source()?;
        let mut stats = FileQueryStats::default();
        let mut io = G15IoStats::default();
        let mut best = None::<f64>;
        let mut stack = vec![0_u32];
        let mut batch = Vec::<u32>::with_capacity(4096);
        while let Some(id) = stack.pop() {
            let node = self.nodes[id as usize];
            stats.nodes_visited += 1;
            if !disk_could_intersect(node.bounds, x, y, radius) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, radius) {
                        continue;
                    }
                    stats.candidates += 1;
                    if stats.candidates % 256 == 0 {
                        g17_check_cancel(cancel)?;
                    }
                    batch.push(p.id);
                    if batch.len() == 4096 {
                        g15_process_batch(
                            &mut self.source,
                            self.records.len(),
                            cache,
                            &batch,
                            x,
                            y,
                            radius,
                            &mut best,
                            &mut stats,
                            &mut io,
                            cancel,
                        )?;
                        batch.clear();
                    }
                }
            }
        }
        if !batch.is_empty() {
            g15_process_batch(
                &mut self.source,
                self.records.len(),
                cache,
                &batch,
                x,
                y,
                radius,
                &mut best,
                &mut stats,
                &mut io,
                cancel,
            )?;
        }
        g17_check_cancel(cancel)?;
        self.check_source()?;
        Ok((best, stats, io))
    }

    /// Utiliza una LRU que se mantiene entre consultas del mismo FileBvh.
    pub fn query_g15_cached(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        self.query_g15_cached_impl(x, y, radius, None)
    }

    fn query_g15_cached_impl(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        g17_check_cancel(cancel)?;
        let mut cache = self
            .block_cache
            .take()
            .ok_or(StlBvhError::InvalidStl("Configure antes la caché G15"))?;
        let result = self.query_g15_with_cache(x, y, radius, &mut cache, cancel);
        self.block_cache = Some(cache);
        result
    }

    fn query_g15_with_cache(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cache: &mut G15BlockCache,
        cancel: Option<&AtomicBool>,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        g17_check_cancel(cancel)?;
        self.check_source()?;
        let mut stats = FileQueryStats::default();
        let mut io = G15IoStats::default();
        let mut best = None::<f64>;
        let mut stack = vec![0_u32];
        while let Some(id) = stack.pop() {
            let node = self.nodes[id as usize];
            stats.nodes_visited += 1;
            if !disk_could_intersect(node.bounds, x, y, radius) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, radius) {
                        continue;
                    }
                    stats.candidates += 1;
                    if stats.candidates % 256 == 0 {
                        g17_check_cancel(cancel)?;
                    }
                    let record =
                        cache.record(&mut self.source, self.records.len(), p.id, &mut io)?;
                    stats.stl_records_read += 1;
                    if let Some(z) =
                        ball_tip_height_triangle(stl_triangle_f64(record)?, x, y, radius)?
                    {
                        best = Some(best.map_or(z, |v| v.max(z)));
                    }
                }
            }
        }
        g17_check_cancel(cancel)?;
        self.check_source()?;
        Ok((best, stats, io))
    }
}

// Se leen bloques próximos en disco, pero las alturas se combinan en el
// orden original del recorrido BVH, conservando su resultado bit a bit.
fn g15_process_batch(
    source: &mut File,
    total_records: usize,
    cache: &mut G15BlockCache,
    ids: &[u32],
    x: f64,
    y: f64,
    radius: f64,
    best: &mut Option<f64>,
    stats: &mut FileQueryStats,
    io: &mut G15IoStats,
    cancel: Option<&AtomicBool>,
) -> Result<(), StlBvhError> {
    let mut order: Vec<usize> = (0..ids.len()).collect();
    order.sort_unstable_by_key(|&i| ids[i]);
    let mut heights = vec![None::<f64>; ids.len()];
    for (position, i) in order.into_iter().enumerate() {
        if position % 256 == 0 {
            g17_check_cancel(cancel)?;
        }
        let record = cache.record(source, total_records, ids[i], io)?;
        heights[i] = ball_tip_height_triangle(stl_triangle_f64(record)?, x, y, radius)?;
        stats.stl_records_read += 1;
    }
    for z in heights.into_iter().flatten() {
        *best = Some(best.map_or(z, |previous| previous.max(z)));
    }
    Ok(())
}

impl FileBvh {
    /// G21: consulta de fresa plana contra el mismo BVH y STL protegido.
    /// Conservar el contrato de cancelacion, estadisticas y presupuesto G16.
    pub fn query_g21_flat_cancelable(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cancel: &AtomicBool,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats, G16Decision), StlBvhError> {
        g17_check_cancel(Some(cancel))?;
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        if !(radius * radius).is_finite() {
            return Err(StlBvhError::ArithmeticRange);
        }
        self.check_source()?;
        let cache = self
            .block_cache
            .as_ref()
            .filter(|_| self.g16_budget.is_some())
            .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
        let (count, warm) = self.g16_sample_candidates(x, y, radius, cache);
        g17_check_cancel(Some(cancel))?;
        let decision = G16Decision {
            mode: g16_choose_mode(count, warm),
            sampled_candidates: count,
            sampled_cached_records: warm,
        };
        let (z, stats, io) = match decision.mode {
            G16AccessMode::Blocks => {
                let mut temporary = G15BlockCache::new(G15_BLOCK_BYTES)?;
                self.query_g21_flat_with_cache(x, y, radius, &mut temporary, cancel)?
            }
            G16AccessMode::Cached => {
                let mut cache = self
                    .block_cache
                    .take()
                    .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
                let result = self.query_g21_flat_with_cache(x, y, radius, &mut cache, cancel);
                self.block_cache = Some(cache);
                result?
            }
        };
        g17_check_cancel(Some(cancel))?;
        Ok((z, stats, io, decision))
    }

    fn query_g21_flat_with_cache(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        cache: &mut G15BlockCache,
        cancel: &AtomicBool,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        g17_check_cancel(Some(cancel))?;
        self.check_source()?;
        let mut stats = FileQueryStats::default();
        let mut io = G15IoStats::default();
        let mut best = None::<f64>;
        let mut stack = vec![0_u32];
        while let Some(id) = stack.pop() {
            let node = self.nodes[id as usize];
            stats.nodes_visited += 1;
            if !disk_could_intersect(node.bounds, x, y, radius) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, radius) {
                        continue;
                    }
                    stats.candidates += 1;
                    if stats.candidates % 256 == 0 {
                        g17_check_cancel(Some(cancel))?;
                    }
                    let record =
                        cache.record(&mut self.source, self.records.len(), p.id, &mut io)?;
                    stats.stl_records_read += 1;
                    if let Some(z) =
                        flat_tip_height_triangle(stl_triangle_f64(record)?, x, y, radius)?
                    {
                        best = Some(best.map_or(z, |previous| previous.max(z)));
                    }
                }
            }
        }
        g17_check_cancel(Some(cancel))?;
        self.check_source()?;
        Ok((best, stats, io))
    }
}

impl FileBvh {
    /// G22: offset euclideo positivo de fresa plana, sin alterar ruta G21 a=0.
    /// El BVH se recorta a r+a, con la misma politica G16 y presupuesto.
    pub fn query_g22_flat_allowance_cancelable(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        allowance: f64,
        cancel: &AtomicBool,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats, G16Decision), StlBvhError> {
        g17_check_cancel(Some(cancel))?;
        if !x.is_finite()
            || !y.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || !allowance.is_finite()
            || allowance <= 0.0
        {
            return Err(ContactError::InvalidInput.into());
        }
        let reach = radius + allowance;
        if !(reach * reach).is_finite() {
            return Err(StlBvhError::ArithmeticRange);
        }
        self.check_source()?;
        let cache = self
            .block_cache
            .as_ref()
            .filter(|_| self.g16_budget.is_some())
            .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
        let (count, warm) = self.g16_sample_candidates(x, y, reach, cache);
        g17_check_cancel(Some(cancel))?;
        let decision = G16Decision {
            mode: g16_choose_mode(count, warm),
            sampled_candidates: count,
            sampled_cached_records: warm,
        };
        let (z, stats, io) = match decision.mode {
            G16AccessMode::Blocks => {
                let mut temporary = G15BlockCache::new(G15_BLOCK_BYTES)?;
                self.query_g22_flat_with_cache(x, y, radius, allowance, &mut temporary, cancel)?
            }
            G16AccessMode::Cached => {
                let mut cache = self
                    .block_cache
                    .take()
                    .ok_or(StlBvhError::InvalidStl("Configure antes G16"))?;
                let result =
                    self.query_g22_flat_with_cache(x, y, radius, allowance, &mut cache, cancel);
                self.block_cache = Some(cache);
                result?
            }
        };
        g17_check_cancel(Some(cancel))?;
        Ok((z, stats, io, decision))
    }

    fn query_g22_flat_with_cache(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        allowance: f64,
        cache: &mut G15BlockCache,
        cancel: &AtomicBool,
    ) -> Result<(Option<f64>, FileQueryStats, G15IoStats), StlBvhError> {
        g17_check_cancel(Some(cancel))?;
        self.check_source()?;
        let reach = radius + allowance;
        let mut stats = FileQueryStats::default();
        let mut io = G15IoStats::default();
        let mut best = None::<f64>;
        let mut stack = vec![0_u32];
        while let Some(id) = stack.pop() {
            let node = self.nodes[id as usize];
            stats.nodes_visited += 1;
            if !disk_could_intersect(node.bounds, x, y, reach) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, reach) {
                        continue;
                    }
                    stats.candidates += 1;
                    if stats.candidates % 256 == 0 {
                        g17_check_cancel(Some(cancel))?;
                    }
                    let record =
                        cache.record(&mut self.source, self.records.len(), p.id, &mut io)?;
                    stats.stl_records_read += 1;
                    if let Some(z) = flat_tip_height_triangle_allowance(
                        stl_triangle_f64(record)?,
                        x,
                        y,
                        radius,
                        allowance,
                    )? {
                        best = Some(best.map_or(z, |previous| previous.max(z)));
                    }
                }
            }
        }
        g17_check_cancel(Some(cancel))?;
        self.check_source()?;
        Ok((best, stats, io))
    }
}

impl FileBvh {
    pub fn query(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<(Option<f64>, FileQueryStats), StlBvhError> {
        if !x.is_finite() || !y.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(ContactError::InvalidInput.into());
        }
        self.check_source()?;
        let mut stats = FileQueryStats::default();
        let mut best = None::<f64>;
        let mut stack = vec![0_u32];
        let mut record = [0_u8; 50];
        while let Some(id) = stack.pop() {
            let node = self.nodes[id as usize];
            stats.nodes_visited += 1;
            if !disk_could_intersect(node.bounds, x, y, radius) {
                continue;
            }
            if node.left != NO_CHILD {
                stack.push(node.right);
                stack.push(node.left);
            } else {
                for p in &self.records[node.start as usize..node.end as usize] {
                    if !disk_could_intersect(p.bounds(), x, y, radius) {
                        continue;
                    }
                    stats.candidates += 1;
                    self.source
                        .seek(SeekFrom::Start(84 + u64::from(p.id) * 50))?;
                    self.source.read_exact(&mut record)?;
                    stats.stl_records_read += 1;
                    if let Some(z) =
                        ball_tip_height_triangle(stl_triangle_f64(&record)?, x, y, radius)?
                    {
                        best = Some(best.map_or(z, |v| v.max(z)));
                    }
                }
            }
        }
        self.check_source()?;
        Ok((best, stats))
    }
    fn check_source(&self) -> Result<(), StlBvhError> {
        if source_stamp(&self.source.metadata()?) == self.original_stamp {
            Ok(())
        } else {
            Err(StlBvhError::SourceChanged)
        }
    }
    pub fn triangle_count(&self) -> usize {
        self.records.len()
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn allocated_bytes(&self) -> usize {
        self.records.capacity() * std::mem::size_of::<PrimitiveRecord>()
            + self.nodes.capacity() * std::mem::size_of::<FileBvhNode>()
    }
}

#[derive(Clone, Debug)]
pub struct BuildControl {
    pub max_triangles: usize,
    pub max_index_bytes: usize,
    pub cancel_file: Option<std::path::PathBuf>,
}
impl BuildControl {
    fn check_g17(&self, cancel: Option<&AtomicBool>) -> Result<(), StlBvhError> {
        g17_check_cancel(cancel)?;
        if self.cancel_file.as_ref().is_some_and(|p| p.exists()) {
            return Err(StlBvhError::InvalidStl(
                "Construcción cancelada por fichero centinela",
            ));
        }
        Ok(())
    }
    fn preflight(&self, count: usize, cancel: Option<&AtomicBool>) -> Result<usize, StlBvhError> {
        self.check_g17(cancel)?;
        if count == 0 || count > self.max_triangles {
            return Err(StlBvhError::InvalidStl("Se supera el límite de triángulos"));
        }
        // Construcción por partición mediana, profundidad logarítmica.
        let leaves = count
            .div_ceil(BVH_LEAF_SIZE)
            .checked_next_power_of_two()
            .ok_or(StlBvhError::ArithmeticRange)?;
        let node_upper = leaves
            .checked_mul(2)
            .and_then(|x| x.checked_sub(1))
            .ok_or(StlBvhError::ArithmeticRange)?;
        let bytes = count
            .checked_mul(std::mem::size_of::<PrimitiveRecord>())
            .and_then(|x| {
                node_upper
                    .checked_mul(std::mem::size_of::<FileBvhNode>())
                    .and_then(|y| x.checked_add(y))
            })
            .ok_or(StlBvhError::ArithmeticRange)?;
        if bytes > self.max_index_bytes {
            return Err(StlBvhError::InvalidStl(
                "Se supera el presupuesto de memoria del índice",
            ));
        }
        Ok(node_upper)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct BuildStats {
    pub bounds_primitives: u64,
    pub partitions: u64,
    pub cancel_checks: u64,
    pub read_ms: f64,
    pub tree_ms: f64,
}

impl FileBvh {
    pub fn build(path: &Path, control: &BuildControl) -> Result<(Self, BuildStats), StlBvhError> {
        Self::build_g17_impl(path, control, None)
    }

    /// Construcción con token atómico; comprueba durante el escaneo y el árbol.
    pub fn build_g17_cancelable(
        path: &Path,
        control: &BuildControl,
        cancel: &AtomicBool,
    ) -> Result<(Self, BuildStats), StlBvhError> {
        Self::build_g17_impl(path, control, Some(cancel))
    }

    fn build_g17_impl(
        path: &Path,
        control: &BuildControl,
        cancel: Option<&AtomicBool>,
    ) -> Result<(Self, BuildStats), StlBvhError> {
        use std::time::Instant;
        control.check_g17(cancel)?;
        let file = open_read_protected(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(StlBvhError::InvalidStl("No es archivo regular"));
        }
        let stamp = source_stamp(&metadata);
        if stamp.0 < 84 {
            return Err(StlBvhError::InvalidStl("Cabecera STL incompleta"));
        }
        let mut reader = BufReader::with_capacity(1024 * 1024, file);
        let mut header = [0_u8; 84];
        reader.read_exact(&mut header)?;
        let count = u32::from_le_bytes(header[80..84].try_into().unwrap()) as usize;
        let node_upper = control.preflight(count, cancel)?;
        if 84_u64 + count as u64 * 50 != stamp.0 {
            return Err(StlBvhError::InvalidStl(
                "Tamaño STL incompatible con número de triángulos",
            ));
        }
        let mut stats = BuildStats::default();
        let t_read = Instant::now();
        let mut records = Vec::new();
        records
            .try_reserve_exact(count)
            .map_err(|_| StlBvhError::ArithmeticRange)?;
        let mut record = [0_u8; 50];
        for id in 0..count {
            if id % 8192 == 0 {
                control.check_g17(cancel)?;
                stats.cancel_checks += 1;
            }
            reader.read_exact(&mut record)?;
            let triangle = stl_triangle_f32(&record)?;
            records.push(PrimitiveRecord::from_tri(&triangle, id as u32));
        }
        let source = reader.into_inner();
        if source_stamp(&source.metadata()?) != stamp {
            return Err(StlBvhError::SourceChanged);
        }
        stats.read_ms = t_read.elapsed().as_secs_f64() * 1000.0;
        control.check_g17(cancel)?;
        stats.cancel_checks += 1;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(node_upper)
            .map_err(|_| StlBvhError::ArithmeticRange)?;
        let mut result = Self {
            source,
            original_stamp: stamp,
            records,
            nodes,
            block_cache: None,
            g16_budget: None,
        };
        let mut until_check = 0usize;
        let t_tree = Instant::now();
        result.build_range(0, count, control, cancel, &mut until_check, &mut stats)?;
        stats.tree_ms = t_tree.elapsed().as_secs_f64() * 1000.0;
        result.check_source()?;
        control.check_g17(cancel)?;
        stats.cancel_checks += 1;
        Ok((result, stats))
    }

    fn build_range(
        &mut self,
        start: usize,
        end: usize,
        control: &BuildControl,
        cancel: Option<&AtomicBool>,
        until_check: &mut usize,
        stats: &mut BuildStats,
    ) -> Result<u32, StlBvhError> {
        // Lectura de límites en bloques: una consulta de cancelación cada
        // ~262 144 primitivas examinadas a lo largo de TODO el árbol.
        let mut bounds = self.records[start].bounds();
        let mut i = start + 1;
        while i < end {
            let chunk_end = i.saturating_add(4096).min(end);
            for p in &self.records[i..chunk_end] {
                bounds = union_f32_bounds(bounds, p.bounds());
            }
            let traversed = chunk_end - i;
            stats.bounds_primitives += traversed as u64;
            *until_check += traversed;
            if *until_check >= 262_144 {
                control.check_g17(cancel)?;
                stats.cancel_checks += 1;
                *until_check = 0;
            }
            i = chunk_end;
        }
        stats.bounds_primitives += 1;
        let node_idx = self.nodes.len();
        let node_id = u32::try_from(node_idx).map_err(|_| StlBvhError::ArithmeticRange)?;
        self.nodes.push(FileBvhNode::leaf(bounds, start, end));
        if end - start > BVH_LEAF_SIZE {
            let dx = f64::from(bounds[1]) - f64::from(bounds[0]);
            let dy = f64::from(bounds[3]) - f64::from(bounds[2]);
            let axis = if dx >= dy { 0 } else { 1 };
            let mid = start + (end - start) / 2;
            // La ordenación conserva el desempate por ID de G06.
            self.records[start..end].select_nth_unstable_by(mid - start, |a, b| {
                a.centroid(axis)
                    .total_cmp(&b.centroid(axis))
                    .then_with(|| a.id.cmp(&b.id))
            });
            stats.partitions += 1;
            // La partición std no admite cancelación en su interior.
            // Comprobamos periódicamente después de particiones. No se puede
            // interrumpir una partición individual con esta API.
            if stats.partitions % 16384 == 0 {
                control.check_g17(cancel)?;
                stats.cancel_checks += 1;
            }
            let left = self.build_range(start, mid, control, cancel, until_check, stats)?;
            let right = self.build_range(mid, end, control, cancel, until_check, stats)?;
            self.nodes[node_idx].left = left;
            self.nodes[node_idx].right = right;
        }
        Ok(node_id)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::cam_geometry::ball_tip_height_mesh;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct TempStl(std::path::PathBuf);
    impl Drop for TempStl {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn write_stl(triangles: &[[[f32; 3]; 3]]) -> TempStl {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("lagucam_g13_{}_{}.stl", std::process::id(), n));
        let mut bytes = vec![0_u8; 80];
        bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for tri in triangles {
            bytes.extend_from_slice(&[0_u8; 12]);
            for p in tri {
                for &v in p {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            bytes.extend_from_slice(&[0_u8; 2]);
        }
        std::fs::write(&path, bytes).unwrap();
        TempStl(path)
    }
    fn control() -> BuildControl {
        BuildControl {
            max_triangles: 100,
            max_index_bytes: 1_000_000,
            cancel_file: None,
        }
    }
    #[test]
    fn g16_selector_uses_candidate_count_and_warmth_not_radius() {
        assert_eq!(g16_choose_mode(100, 0), G16AccessMode::Blocks);
        assert_eq!(g16_choose_mode(6888, 0), G16AccessMode::Blocks);
        assert_eq!(g16_choose_mode(16384, 0), G16AccessMode::Cached);
        assert_eq!(g16_choose_mode(6888, 6000), G16AccessMode::Cached);
        assert_eq!(g16_choose_mode(6888, 100), G16AccessMode::Blocks);
    }

    #[test]
    fn g16_budget_includes_map_overhead_and_query_workspace() {
        let file = write_stl(&[[[0., 0., 1.], [1., 0., 1.], [0., 1., 1.]]]);
        let (mut bvh, _) = FileBvh::build(&file.0, &control()).unwrap();
        let payload = G15_BLOCK_BYTES;
        let lower = bvh.allocated_bytes() + payload;
        assert!(bvh.configure_g16_auto_cache(payload, lower).is_err());
        assert!(bvh.g16_memory_estimate().is_none());
        let estimate = bvh
            .configure_g16_auto_cache(payload, 2 * 1024 * 1024)
            .unwrap();
        assert!(estimate.cache_table_allowance_bytes > 0);
        assert!(estimate.temporary_allowance_bytes > 0);
        assert!(estimate.accounted_bytes > lower);
        assert!(estimate.accounted_bytes <= estimate.configured_budget_bytes);
        assert_eq!(bvh.g16_memory_estimate(), Some(estimate));
    }

    #[test]
    fn g16_auto_matches_baseline_on_cold_and_warm_cache() {
        let triangles: Vec<[[f32; 3]; 3]> = (0..2200)
            .map(|i| [[0.0, 0.0, (i as f32) / 8.0]; 3])
            .collect();
        let file = write_stl(&triangles);
        let mut settings = control();
        settings.max_triangles = 3000;
        let (mut bvh, _) = FileBvh::build(&file.0, &settings).unwrap();
        bvh.configure_g16_auto_cache(G15_BLOCK_BYTES * 3, 2 * 1024 * 1024)
            .unwrap();
        let reference = bvh.query(0.0, 0.0, 1.0).unwrap();
        let first = bvh.query_g16_auto(0.0, 0.0, 1.0).unwrap();
        assert_eq!(first.3.mode, G16AccessMode::Blocks);
        assert_eq!(first.0.map(f64::to_bits), reference.0.map(f64::to_bits));
        assert_eq!(first.1, reference.1);
        // Precargamos las entradas con una consulta LRU y repetimos.
        let _ = bvh.query_g15_cached(0.0, 0.0, 1.0).unwrap();
        let warm = bvh.query_g16_auto(0.0, 0.0, 1.0).unwrap();
        assert_eq!(warm.3.mode, G16AccessMode::Cached);
        assert_eq!(warm.0.map(f64::to_bits), reference.0.map(f64::to_bits));
        assert_eq!(warm.1, reference.1);
        assert!(warm.2.cache_hits > 0);
    }

    #[test]
    fn g16_large_density_selects_cache_without_large_radius() {
        // 16 400 triángulos coincidentes en XY: más candidatos, mismo radio.
        let triangles = vec![[[0.0, 0.0, 1.0]; 3]; 16_400];
        let file = write_stl(&triangles);
        let mut settings = control();
        settings.max_triangles = 20_000;
        let (mut bvh, _) = FileBvh::build(&file.0, &settings).unwrap();
        bvh.configure_g16_auto_cache(G15_BLOCK_BYTES * 20, 4 * 1024 * 1024)
            .unwrap();
        let (z, stats, _, decision) = bvh.query_g16_auto(0.0, 0.0, 0.25).unwrap();
        assert_eq!(decision.mode, G16AccessMode::Cached);
        assert_eq!(stats.candidates, triangles.len());
        assert_eq!(z, Some(1.0));
    }

    #[test]
    fn g15_blocks_and_cache_match_individual_at_block_boundaries() {
        // 2050 registros: dos bloques completos y un tercero parcial.
        let triangles: Vec<[[f32; 3]; 3]> = (0..2050)
            .map(|i| {
                let x = i as f32;
                let z = (i as f32) / 8.0;
                [[x, 0.0, z], [x, 0.0, z], [x, 0.0, z]]
            })
            .collect();
        let file = write_stl(&triangles);
        let mut settings = control();
        settings.max_triangles = 3000;
        let (mut bvh, _) = FileBvh::build(&file.0, &settings).unwrap();
        bvh.configure_g15_cache(G15_BLOCK_BYTES, 1_000_000).unwrap();
        for &id in &[0, 1, 1023, 1024, 2047, 2048, 2049] {
            let (x, y, r) = (id as f64, 0.0, 0.25);
            let reference = bvh.query(x, y, r).unwrap();
            let grouped = bvh.query_g15_blocks(x, y, r).unwrap();
            let cached = bvh.query_g15_cached(x, y, r).unwrap();
            assert_eq!(reference.0.map(f64::to_bits), grouped.0.map(f64::to_bits));
            assert_eq!(reference.0.map(f64::to_bits), cached.0.map(f64::to_bits));
            assert_eq!(reference.1, grouped.1);
            assert_eq!(reference.1, cached.1);
            assert_eq!(
                grouped.2.cache_hits + grouped.2.cache_misses,
                grouped.1.candidates
            );
            assert_eq!(
                cached.2.cache_hits + cached.2.cache_misses,
                cached.1.candidates
            );
        }
    }

    #[test]
    fn g15_grouped_reads_each_contiguous_block_only_once() {
        let triangles: Vec<[[f32; 3]; 3]> = (0..2200)
            .map(|i| [[0.0, 0.0, (i as f32) / 8.0]; 3])
            .collect();
        let file = write_stl(&triangles);
        let mut settings = control();
        settings.max_triangles = 3000;
        let (mut bvh, _) = FileBvh::build(&file.0, &settings).unwrap();
        let individual = bvh.query(0.0, 0.0, 1.0).unwrap();
        let grouped = bvh.query_g15_blocks(0.0, 0.0, 1.0).unwrap();
        assert_eq!(individual.0.map(f64::to_bits), grouped.0.map(f64::to_bits));
        assert_eq!(individual.1, grouped.1);
        assert_eq!(grouped.1.candidates, 2200);
        assert_eq!(grouped.2.block_reads, 3);
        assert_eq!(grouped.2.cache_hits, 2197);
        assert_eq!(grouped.2.bytes_read, 2200 * 50);
    }

    #[test]
    fn g15_cache_reuses_block_and_evicts_oldest() {
        let triangles: Vec<[[f32; 3]; 3]> = (0..2050).map(|i| [[i as f32, 0.0, 1.0]; 3]).collect();
        let file = write_stl(&triangles);
        let mut settings = control();
        settings.max_triangles = 3000;
        let (mut bvh, _) = FileBvh::build(&file.0, &settings).unwrap();
        bvh.configure_g15_cache(G15_BLOCK_BYTES, 1_000_000).unwrap();
        let first = bvh.query_g15_cached(0.0, 0.0, 0.25).unwrap();
        let second = bvh.query_g15_cached(1.0, 0.0, 0.25).unwrap();
        let third = bvh.query_g15_cached(1024.0, 0.0, 0.25).unwrap();
        let fourth = bvh.query_g15_cached(0.0, 0.0, 0.25).unwrap();
        assert_eq!(first.2.block_reads, 1);
        assert_eq!(second.2.cache_hits, 1);
        assert_eq!(second.2.block_reads, 0);
        assert_eq!(third.2.block_reads, 1);
        assert_eq!(fourth.2.block_reads, 1);
    }

    #[test]
    fn g15_cache_capacity_and_budget_are_enforced() {
        let file = write_stl(&[[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]]);
        let (mut bvh, _) = FileBvh::build(&file.0, &control()).unwrap();
        assert!(bvh.query_g15_cached(0.0, 0.0, 1.0).is_err());
        assert!(
            bvh.configure_g15_cache(G15_BLOCK_BYTES - 1, 1_000_000)
                .is_err()
        );
        assert!(
            bvh.configure_g15_cache(G15_MAX_CACHE_BYTES + 1, usize::MAX)
                .is_err()
        );
        assert!(bvh.configure_g15_cache(G15_BLOCK_BYTES, 1).is_err());
        assert_eq!(bvh.g15_cache_capacity_bytes(), 0);
        bvh.configure_g15_cache(G15_BLOCK_BYTES * 2, 1_000_000)
            .unwrap();
        assert_eq!(bvh.g15_cache_capacity_bytes(), G15_BLOCK_BYTES * 2);
    }

    #[cfg(windows)]
    #[test]
    fn g15_windows_read_handle_rejects_new_writer() {
        let file = write_stl(&[[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]]);
        let (_bvh, _) = FileBvh::build(&file.0, &control()).unwrap();
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file.0)
                .is_err()
        );
    }

    #[test]
    fn single_triangle_matches_independent_reference() {
        let t = [[[0., 0., 5.], [10., 0., 5.], [0., 10., 5.]]];
        let file = write_stl(&t);
        let (mut bvh, _) = FileBvh::build(&file.0, &control()).unwrap();
        assert_eq!(bvh.triangle_count(), 1);
        assert_eq!(bvh.query(2., 2., 1.).unwrap().0, Some(5.));
        assert_eq!(bvh.query(50., 50., 1.).unwrap().0, None);
    }
    #[test]
    fn bvh_matches_exhaustive_for_sloped_and_vertical_triangles() {
        let tris = [
            [[0., 0., 0.], [10., 0., 5.], [0., 10., 2.]],
            [[4., 4., 0.], [4., 4., 12.], [4., 10., 3.]],
            [[20., 20., 10.], [25., 20., 10.], [20., 25., 10.]],
        ];
        let file = write_stl(&tris);
        let (mut bvh, _) = FileBvh::build(&file.0, &control()).unwrap();
        let all: Vec<_> = tris
            .iter()
            .map(|t| {
                t.map(|p| Vec3 {
                    x: p[0] as f64,
                    y: p[1] as f64,
                    z: p[2] as f64,
                })
            })
            .collect();
        for (x, y, r) in [
            (4., 4., 2.),
            (12., 12., 3.),
            (22., 22., 5.),
            (100., 100., 1.),
        ] {
            assert_eq!(
                bvh.query(x, y, r).unwrap().0,
                ball_tip_height_mesh(&all, x, y, r).unwrap()
            );
        }
    }
    #[test]
    fn rejects_memory_budget_before_index_allocation() {
        let file = write_stl(&[[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]]);
        let mut c = control();
        c.max_index_bytes = 1;
        assert!(FileBvh::build(&file.0, &c).is_err());
    }
    #[test]
    fn detects_cancellation_before_build() {
        let file = write_stl(&[[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]]);
        let stop = std::env::temp_dir().join(format!(
            "lagucam_g13_stop_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&stop, b"").unwrap();
        let mut c = control();
        c.cancel_file = Some(stop.clone());
        assert!(FileBvh::build(&file.0, &c).is_err());
        std::fs::remove_file(stop).unwrap();
    }
}

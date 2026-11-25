//! Ejecucion asicrona del barrido NC; la interfaz nunca llama a SparseMaterial::seek.
use cam_libre_visor::{
    material_sim::CutSegment,
    sparse_material::{MaterialRegion, SparseMaterial},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};

pub struct MaterialFrame {
    pub cursor: f64,
    pub regions: Vec<MaterialRegion>,
}

pub struct SparseRuntime {
    desired: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<MaterialFrame, String>>,
}

impl SparseRuntime {
    pub fn new(mut field: SparseMaterial, segments: Vec<CutSegment>) -> Result<Self, String> {
        let desired = Arc::new(AtomicU64::new(0.0_f64.to_bits()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let wanted = Arc::clone(&desired);
        let stop = Arc::clone(&cancel);
        thread::Builder::new()
            .name("lagucam-material-worker".to_string())
            .spawn(move || {
                let mut previous = 0.0;
                while !stop.load(Ordering::Relaxed) {
                    let target = f64::from_bits(wanted.load(Ordering::Relaxed));
                    if target != previous {
                        match field.seek(&segments, target) {
                            Ok(()) => {
                                previous = target;
                                let regions = field.take_dirty_regions();
                                if tx
                                    .send(Ok(MaterialFrame {
                                        cursor: target,
                                        regions,
                                    }))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(error) => {
                                let _ = tx.send(Err(error));
                                break;
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(8));
                    }
                }
            })
            .map_err(|error| format!("No se pudo iniciar trabajador de material: {error}"))?;
        Ok(Self {
            desired,
            cancel,
            rx,
        })
    }

    pub fn request(&self, cursor: f64) {
        if cursor.is_finite() {
            self.desired.store(cursor.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn receive(&self) -> Result<Option<MaterialFrame>, String> {
        match self.rx.try_recv() {
            Ok(Ok(frame)) => Ok(Some(frame)),
            Ok(Err(error)) => Err(error),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("Trabajador de material desconectado".into()),
        }
    }
}

impl Drop for SparseRuntime {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

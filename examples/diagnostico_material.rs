//! Diagnostico aislado: no modifica el motor de simulacion ni el visor.
use cam_libre_visor::material_sim::{
    CutSegment, CutterShape, CutterSpec, MaterialHeightfield, StockBox,
};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Instant;

fn main() {
    let step_mm: f64 = 0.5;
    println!("DIAGNOSTICO LAGUCAM - MATERIAL 2.5D");
    println!("Paso objetivo: {step_mm:.3} mm | Fresa plana: 5 mm");
    println!("Tiempos orientativos: ejecucion RELEASE, sin render OpenGL");
    println!("Pieza(mm) | Malla | Paso(mm) | Tiempo(s) | Historial MiB | Cambios | Volumen(mm3)");
    let cutter = CutterSpec {
        shape: CutterShape::Flat,
        diameter_mm: 5.0,
        cutting_length_mm: 12.0,
    };
    for size in [100.0_f64, 200.0, 400.0, 800.0] {
        let resolution = (size / step_mm).ceil() as usize + 1;
        if resolution > 1024 {
            println!("{size:>8.0} | {resolution:>5} | -- BLOQUEADO: limite del motor actual (1024)");
            continue;
        }
        let stock = StockBox {
            min_mm: [0.0, 0.0, 0.0],
            size_mm: [size, size, 10.0],
        };
        let segments: Vec<_> = (0..8).map(|i| {
            let y = size * (0.15 + i as f64 * 0.1);
            CutSegment {
                from_model_mm: [size * 0.1, y, 5.0],
                to_model_mm: [size * 0.9, y, 5.0],
                spindle_on: true,
                rapid: false,
                nc_line: i + 1,
            }
        }).collect();
        let start = Instant::now();
        match MaterialHeightfield::simulate_with_budget(
            stock, cutter, resolution, 256, &segments,
            &AtomicBool::new(false), &AtomicUsize::new(0),
        ) {
            Ok(mut field) => {
                let elapsed = start.elapsed().as_secs_f64();
                field.seek(segments.len() as f64);
                let dx = size / (field.cells[0] - 1) as f64;
                println!("{size:>8.0} | {:>4}x{:<4} | {dx:>8.3} | {elapsed:>9.3} | {:>12.2} | {:>7} | {:>12.1}",
                    field.cells[0], field.cells[1],
                    field.history_memory_bytes() as f64 / 1_048_576.0,
                    field.history_len(), field.removed_mm3());
            }
            Err(error) => {
                println!("{size:>8.0} | {resolution:>5} | ERROR: {error}");
            }
        }
    }
    println!("FIN. No extrapolar el tiempo de esta prueba al NC real.");
}
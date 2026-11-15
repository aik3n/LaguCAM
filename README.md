# LaguCAM

**CAM libre experimental para fresado CNC.** Licencia MIT. Repositorio: https://github.com/aik3n/LaguCAM.

LaguCAM comienza con relieves topográficos representados en STL binarios grandes y una primera estrategia de acabado paralelo de tres ejes. La arquitectura pretende incorporar más estrategias, máquinas y formatos geométricos en fases posteriores.

> **Estado vigente, evidencias y SHA de referencia:** [`docs/STATUS.md`](docs/STATUS.md). El README explica capacidades generales; no mantiene otra copia del estado por commit.

## Qué ofrece el prototipo

- Visor STL con carga en bloques, navegación 3D y pivote visual sobre la superficie; el responsable ha probado la navegación de un STL real cercano a 1 GB, **sin benchmark de rendimiento formal**.
- Proyectos `.lagucam` v2 con referencia a STL externo, bruto, origen y operaciones configurables.
- Biblioteca global de cinco geometrías de herramienta, UUID, revisiones y función SHA-256 de geometría física; la integración del hash con trayectorias está pendiente.
- Motor geométrico experimental, independiente del render, con contacto de punta esférica sobre triángulos originales y aceleración espacial BVH.
- Estrategia experimental **«Acabado paralelo — TEST»**: muestras de contacto y previsualización 3D, parámetros y zona XY, cancelación y guardado de configuración. **No son movimientos CNC verificados.**
- Paneles reordenables, temas claro/oscuro y configuración portable junto al ejecutable.

**No existe todavía exportación de G-code ni un flujo de mecanizado seguro y verificado.** No hay comprobación integral de colisiones, movimientos de entrada/retirada ni postprocesador FluidNC implementado. No utilizar una previsualización como programa de mecanizado.

## Compilación

Requiere Rust según `rust-toolchain.toml`, entorno gráfico compatible con egui/eframe y OpenGL/glow.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

El paquete y ejecutable Cargo siguen denominándose `cam-libre-visor` (`cam-libre-visor.exe` en Windows). No interpretar esta lista de comandos como prueba de que CI haya sido configurado o ejecutado.

## Persistencia y datos

La configuración portable se guarda en `config/tools.json`, `config/settings.json` y `config/panels.json` al lado del ejecutable. Los proyectos `.lagucam` hacen referencia al STL externo; no duplican la malla ni la definición editable de las fresas. Durante el desarrollo **no se exige compatibilidad retroactiva de formatos**, pero jamás deben destruirse datos personales sin autorización.

Los STL binarios almacenan vértices `f32`; los cálculos del motor operan con esos valores convertidos a `f64`. No recuperan precisión perdida en el archivo original. La malla de GPU normalizada no sirve para el cálculo CAM.

## Documentación

- [Estado actual y validación](docs/STATUS.md)
- [Índice documental](docs/INDEX.md)
- [Arquitectura](docs/ARCHITECTURE.md)
- [Decisiones](docs/DECISIONS.md)
- [Plan de desarrollo](docs/ROADMAP.md)
- [Pruebas y seguridad](docs/TESTING.md)
- [Proceso de trabajo](docs/WORKFLOW.md)

Si el último SHA de GitHub es posterior al indicado en `docs/STATUS.md`, hay cambios aún **no sincronizados documentalmente**; revisar esos commits antes de tomar decisiones.

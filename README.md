# LaguCAM

**CAM libre (MIT) para fresado CNC**, desarrollado en Rust, con interfaz egui/eframe y visor OpenGL. El primer objetivo es un flujo de **acabado paralelo de 3 ejes sobre STL**, preparado para mallas grandes y ampliable a otras estrategias y formatos en el futuro.

Repositorio: https://github.com/aik3n/LaguCAM

## Retomar el proyecto en un chat nuevo

Leer, en este orden:

1. **[docs/CONTEXTO.md](docs/CONTEXTO.md)** — objetivo, capacidades observadas, decisiones vigentes, límites y siguiente trabajo.
2. **[docs/RESPONSABILIDADES_CAM_NC.md](docs/RESPONSABILIDADES_CAM_NC.md)** — contratos entre estrategia, planificador, post, simulación, UI y operador.
3. **[AGENTS.md](AGENTS.md)** — reglas de trabajo y entrega de cambios.

Después, consultar el **código del commit o de la copia local** antes de modificarlo. La documentación no sustituye a la inspección del código ni acredita pruebas ejecutadas.

## Estado funcional resumido

El código incluye visor STL, proyectos `.lagucam`, bruto y origen, biblioteca global de fresas, acabado paralelo experimental, planificación de operaciones, generación e interpretación NC, verificación parcial y simulación orientativa Heightfield. La simulación dispone de **resultado final** y **reproducción progresiva** sobre un mismo motor 2,5D.

**No está certificado para mecanizado real:** la simulación y las verificaciones actuales no acreditan ausencia de colisiones ni seguridad física de la CNC.

## Entorno de desarrollo

- Windows, PowerShell 7 y Rust según `rust-toolchain.toml`.
- Carpeta de proyecto: `$HOME\Desktop\LaguCAM` (sin usuario ni unidad fijos).
- Ejecutable Cargo: `cam-libre-visor.exe` (el nombre interno del paquete sigue siendo `cam-libre-visor`).

```powershell
$proyecto = Join-Path $HOME 'Desktop\LaguCAM'
Set-Location $proyecto
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --lib --bins --tests
cargo build --locked --release
```

Estos son comandos sugeridos, **no resultados acreditados**. La ejecución de `cargo test --locked --all-targets` también incluye ejemplos de diagnóstico y debe revisarse si fallan por errores propios de esos ejemplos.

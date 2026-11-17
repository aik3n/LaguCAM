# LaguCAM

**Prototipo experimental de CAM libre (MIT)** para fresado CNC. Código público: https://github.com/aik3n/LaguCAM.

Objetivo inicial: **acabado paralelo de tres ejes sobre STL**, con arquitectura extensible a otras estrategias, máquinas y formatos más adelante. Consultar siempre [`docs/STATUS.md`](docs/STATUS.md) para el SHA inspeccionado, evidencia y límites.

## Capacidades observadas en el código (`5cd41c7`, 2026-11-17)

- Visor STL binario de gran tamaño, navegación, selección de pivote, cubo de orientación y vista ortográfica/perspectiva. El usuario ha probado navegación fluida de un STL cercano a 1 GB; no es un benchmark CAM.
- Proyectos `.lagucam` con referencia externa al STL, bruto, origen de trabajo, operaciones y guardado por reemplazo; recientes en configuración portable. **Sin backups automáticos de proyectos**, por decisión del usuario.
- Biblioteca global de fresas con UUID, SHA-256 geométrico, editor y perfiles; ContactSolver para esférica, plana y toroidal, incluido sobrematerial geométrico.
- Acabado paralelo **TEST**, patrones unidireccional/zigzag, trayectoria neutral y enlaces de aproximación/retirada propuestos, marcadores de inicio/fin y validación **estructural**.
- Contrato CAM con condiciones F/S opcionales y transformación inicial de coordenadas modelo → trabajo para traslación y giro alrededor de Z.

**No está terminado el flujo CNC:** faltan verificación continua de colisiones, perfiles de máquina, postprocesador FluidNC, exportación controlada y validación real. «Correcto estructuralmente» **no** significa seguro para fresar.

## Compilación y pruebas en Windows

Requiere Rust según `rust-toolchain.toml` y entorno compatible con OpenGL/glow. El ejecutable Cargo sigue llamándose `cam-libre-visor.exe`.

```powershell
cd "$env:USERPROFILE\Desktop\LaguCAM"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo check --locked --all-targets
cargo build --locked --release
```

La existencia de estos comandos no significa que hayan sido ejecutados en el HEAD documentado o que exista CI configurada.

## Desarrollo mediante parches locales

**Los chats no realizan operaciones Git.** Cada mejora aceptada se entrega mediante un **script `.ps1` colocado en el Escritorio, junto a la carpeta `LaguCAM`**. El script usa `$PSScriptRoot` para localizar el proyecto; el chat proporciona instrucciones de aplicación, compilación, pruebas y comprobaciones manuales. El usuario ejecuta el parche, valida y realiza **personalmente** los commits y publicaciones. Los chats de programación no editan documentación por iniciativa propia; la sincronización se hace al final del hito.

Reglas exactas: [`AGENTS.md`](AGENTS.md) y [`docs/WORKFLOW.md`](docs/WORKFLOW.md).

## Archivos y datos

Configuración portable junto al ejecutable: `config/settings.json`, `config/panels.json`, `config/tools.json` y `config/recent_projects.json`. El proyecto guarda referencia al STL externo, no una copia gigante de la malla ni un snapshot editable de la fresa. Los STL originales almacenan vértices `f32`, decodificados a `f64` para cálculos geométricos; el render normalizado no es autoridad CAM. Durante desarrollo no se exigen migraciones automáticas, pero nunca se destruyen datos personales silenciosamente.

## Documentación

- [Estado, SHA y evidencia](docs/STATUS.md)
- [Índice](docs/INDEX.md)
- [Arquitectura](docs/ARCHITECTURE.md)
- [Decisiones](docs/DECISIONS.md)
- [Hoja de ruta hacia el prototipo](docs/ROADMAP.md)
- [Pruebas y seguridad](docs/TESTING.md)
- [Proceso por parches PS1](docs/WORKFLOW.md)

El SHA de `STATUS.md` es el **último corte documentado**, no necesariamente el de los archivos locales aún no versionados. No atribuir pruebas realizadas sobre un SHA distinto.

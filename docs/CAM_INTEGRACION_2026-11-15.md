# LaguCAM — integración experimental del motor CAM (15-11-2026)

**Naturaleza:** informe histórico del hito, no estado vivo. Estado actualizado en [`STATUS.md`](STATUS.md).

## Trazabilidad

- Base del hito de motor: [`7de83c9`](https://github.com/aik3n/LaguCAM/commit/7de83c9d6d02e820d69688fd74d3b1f64d9586e0) — integración de consultas geométricas y operaciones.
- Consolidación posterior en `main`: [`13a439a`](https://github.com/aik3n/LaguCAM/commit/13a439ab15fb5baf8942bcb87af78c6da59d49fb) — limpieza menor y documentación.
- La rama local `integracion/cam-20261115` fue el entorno de trabajo durante la integración; **los commits citados ya están publicados en `main`**.

## Código incorporado

- Motor STL independiente del render (`src/cam_geometry/*`): cálculos de contacto de fresa esférica con triángulos originales; BVH XY, memoria contabilizada y lectura por bloques.
- Operaciones parametrizables y persistentes: creación sin estrategia, editor por pestañas, selección, borrado y guardado en `.lagucam`.
- Acabado paralelo **TEST**: diámetro de herramienta introducido manualmente, pasos XY, dirección, área de cálculo, trabajo en segundo plano, cancelación y máximo temporal de 12.000 muestras.
- Funciones de diagnóstico geométrico y visualización de secciones/muestras; no representan estrategias definitivas.

## Evidencias disponibles

- Informe de ejecución en Windows relativo a la base `7de83c9`: `cargo fmt --all -- --check` correcto, `cargo test --all-targets` con **130 pruebas superadas** (20 biblioteca, 104 aplicación, 6 contrato G18), `cargo check` y `cargo build --release` correctos, con cuatro avisos no bloqueantes en esa base.
- Prueba comunicada por el responsable tras integrar: la aplicación abre STL y permite crear, editar y eliminar operaciones. Pruebas adicionales de parámetros, zonas y reapertura de proyectos se realizaron en el entorno experimental.
- Estos resultados **no han sido repetidos por este chat** sobre el commit `13a439a`; registrar de nuevo cualquier ejecución posterior.

## Limitaciones a la fecha del hito

- Las líneas dibujadas **conectan muestras** y no verifican el volumen barrido entre ellas. No son trayectorias CNC seguras.
- Sin G-code, FluidNC implementado, movimientos de entrada/retirada, simulación integral, comprobación de colisiones ni certificado de tolerancia geométrica.
- Sin selector real de herramienta desde biblioteca ni comprobación de hash SHA-256 de resultados CAM.
- Sin benchmark acreditado del motor sobre STL real de aproximadamente 1 GB; la apertura fluida del visor no equivale a rendimiento CAM.

No presentar esta integración experimental como un sistema listo para mecanizar.

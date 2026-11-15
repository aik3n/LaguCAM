# Estado verificable de LaguCAM

**Corte documental:** 2026-11-15. **Rama comprobada:** `main`.

| Referencia | Valor |
| --- | --- |
| Código HEAD consultado al preparar este corte | [`13a439ab15fb5baf8942bcb87af78c6da59d49fb`](https://github.com/aik3n/LaguCAM/commit/13a439ab15fb5baf8942bcb87af78c6da59d49fb) |
| Código cubierto por esta documentación | `13a439ab15fb5baf8942bcb87af78c6da59d49fb` |
| Evidencia técnica revisada aquí | Inspección estática de código y cambios GitHub; **sin ejecución propia de Rust, GUI ni CNC** |
| Pruebas automáticas comunicadas | Informe previo del entorno Windows: **130 tests** en el commit de integración `7de83c9`; no ejecución reproducida en esta revisión |
| Pruebas manuales comunicadas | Abrir STL real cercano a 1 GB y navegarlo (fase de visor); abrir STL y crear/editar/eliminar operaciones tras la integración CAM |
| Estado de mecanizado | **No apto para producción:** no hay G-code, postprocesado, colisiones ni trayectorias verificadas |

**Regla de sincronización:** este archivo describe el código hasta el SHA indicado. Si `main` avanza, este documento **no** queda automáticamente al día. Consultar HEAD antes de iniciar una tarea. Es normal que los SHA difieran hasta el cierre documental del siguiente hito. Ver [`WORKFLOW.md`](WORKFLOW.md).

## Capacidades verificables

| Subsistema | Implementación observada | Qué falta acreditar / completar |
| --- | --- | --- |
| STL binario y visor | Lectura en bloques, gestión GPU, cámara, pivote y navegación | Benchmarks medidos, regresión cuantitativa en STL de ~1 GB |
| Proyecto y configuración | `.lagucam` v2, modelo externo, bruto, origen, guardado; preferencias portables | Más validación de proyectos reales y compatibilidad de cambios experimentales |
| Biblioteca de herramientas | Cinco geometrías; UUID/revisión; hash canónico SHA-256 implementado | Resolver herramienta desde operación; comparar hash en resultados CAM |
| Motor geométrico | `src/cam_geometry/`: contacto fresa esférica–triángulo `f64`, índice BVH XY, lectura STL por bloques/caché, límites y cancelación | Validación matemática independiente y rendimiento medido con 20 M de triángulos; alcance de colisiones |
| Operaciones CAM | Crear, editar, eliminar; editor por pestañas; parámetros persistentes | Selección real de herramienta; modelo de dependencias de resultados |
| Acabado paralelo TEST | Consultas de altura por muestras XY; previsualización; cancelación; límite temporal de 12.000 puntos | Garantía de error entre muestras, enlaces seguros, retiradas, colisiones, tolerancias |
| Trayectoria neutral y post | **No implementados** | Diseño y validación antes de generar código ejecutable |
| Máquinas, FluidNC, STEP, 4/5 ejes | **Objetivos futuros** | No confundir con capacidad disponible |

## Hechos técnicos del prototipo CAM

- `src/cam_geometry/mod.rs`: cálculo de altura de contacto de punta esférica sobre caras/aristas/vértices y consulta O(N) de referencia.
- `src/cam_geometry/stl.rs` y `service.rs`: BVH espacial sobre STL original, caché de bloques, presupuesto estimado y fachada de consulta. El límite es de memoria contabilizada **del motor**, no de la RAM/RSS total del proceso.
- `src/test_parallel.rs`: cálculo experimental en hilo trabajador con cancelación. Diámetro introducido manualmente; **no selecciona todavía la fresa global por UUID**. Los puntos dibujados no certifican que los segmentos intermedios sean seguros.
- La fachada del motor de contacto declara soporte de apertura protegida en **Windows**; no presume soporte idéntico en otros sistemas.
- Los STL originales codifican vértices en `f32`. Su conversión a `f64` reduce error aritmético posterior pero no añade precisión de origen.

## Evidencias y sus límites

- En `docs/CAM_INTEGRACION_2026-11-15.md` constan resultados comunicados en Windows: `cargo fmt --check`, `cargo test --all-targets` (130), `cargo check` y `cargo build --release` relativos a la base experimental `7de83c9`.
- El commit de consolidación `13a439a` elimina avisos menores y actualiza documentación; **no hay ejecución independiente de los comandos sobre este SHA acreditada en esta revisión**.
- No se ha comprobado CI activo en el árbol del commit inspeccionado.
- El STL grande del usuario prueba experiencia de navegación del **visor**, no tiempo de construcción del BVH ni precisión del **motor CAM**.

## Próximos criterios de validación (no entregas aprobadas automáticamente)

1. Contrastar el contacto esférico con casos analíticos independientes, superficies degeneradas y aristas, fijando tolerancias numéricas.
2. Medir tiempos de construcción del BVH, consultas y pico de memoria con STL crecientes, incluido el real de ~1 GB, sin inventar métricas.
3. Definir control de error entre muestras y recorridos seguros antes de considerar una trayectoria CNC.
4. Resolver herramientas desde la biblioteca, capturar su geometría durante el cálculo y comprobar el hash al validar resultados; diferenciar cambios geométricos y tecnológicos.
5. Solo después, diseñar trayectorias neutrales, simulación/colisiones y postprocesado FluidNC.

**Decisiones aceptadas:** `docs/DECISIONS.md`. **Fases y pruebas:** `docs/WORKFLOW.md` y `docs/TESTING.md`.

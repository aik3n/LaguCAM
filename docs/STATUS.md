# Estado verificable de LaguCAM

<!-- LAGUCAM-MASTER-DOC-CUT: b5b15248ecf357c005dc2fadb7388e545d25cfa7 -->

**Corte documental:** 2026-11-19. **Código público inspeccionado:** [`b5b15248ecf357c005dc2fadb7388e545d25cfa7`](https://github.com/aik3n/LaguCAM/commit/b5b15248ecf357c005dc2fadb7388e545d25cfa7) (`main` en la consulta). Comparación de `6325e521` a `b5b15248` realizada en lectura; la carpeta local del usuario no ha sido inspeccionada directamente por el Chat Master.

## 1. Evidencia del corte

| Evidencia | Resultado y procedencia |
| --- | --- |
| Código | Inspección estática de `src/cam_plan.rs`, `src/nc_post.rs`, contratos y pruebas en el commit `b5b15248` |
| Pruebas Rust en Windows | **379 superadas, 0 fallos**, reportadas por el usuario desde `Escritorio/LaguCAM` mediante `cargo test --locked --all-targets` (129 lib, 211 main y 39 de integración) |
| Regresión NC | **9/9 pruebas** de `tests/nc_batch_equivalence.rs` superadas dentro del total anterior; incluyen referencias literales, Start/End, T/M6, precisión, unidades, finales de línea y precedencia de errores |
| Formato | `cargo fmt --all -- --check` sin salida de error, reportado por el usuario |
| Análisis estático | `cargo clippy --locked --all-targets -- -D warnings` finalizado sin advertencias, reportado por el usuario |
| Compilación optimizada | `cargo build --locked --release` finalizado correctamente, reportado por el usuario |
| Alcance de la evidencia | Los comandos se ejecutaron en el árbol local después de presentar `b5b15248`; **no hay una comprobación independiente del SHA exacto del árbol local**. Este chat no ejecutó Cargo en Windows |
| Sin acreditar | Prueba GUI integral, mecanizado real, simulación conjunta de material, benchmark de STL grande y `cargo check --locked --all-targets` como comando independiente |
| Publicación documental | Este corte solo existirá en el repositorio tras aplicar el parche documental y realizar commit/push por parte del usuario |

## 2. Capacidades implementadas inspeccionadas hasta `b5b15248`

| Área | Estado presente en código | Limitación actual |
| --- | --- | --- |
| Proyecto y visor | STL externo, OpenGL, bruto, origen CNC, `.lagucam` v2, unidades y transformaciones no destructivas | Sin prueba integral de sincronización visor/BVH y rendimiento sobre STL grande en este corte |
| Biblioteca y geometría | Herramientas UUID, huella geométrica SHA-256 y ContactSolver plana/esférica/toroidal | No verificadas colisiones de conjunto herramienta, mango, portaherramientas y fijaciones |
| Acabado paralelo TEST | Contactos, sobreespesor, zigzag, aproximaciones, cortes, retiradas y enlaces internos neutrales | No es desbaste ni recorrido CNC certificado |
| Procedencia | `CamProvenance`, identidad y caducidad de resultados, firma económica del STL | La firma del STL no equivale a hash integral ni a validación de máquina |
| Operación individual | Calcular, generar NC, interpretar y reproducir el NC; Start/End admitidos por su contrato | La posición inicial requerida es una precondición teórica del programa, no un estado físico verificado |
| Cálculo de varias operaciones | `cam_batch.rs` coordina activas en orden, reutilización, cancelación y reordenación | Faltan pruebas manuales integrales de ediciones concurrentes y cancelación |
| NC conjunto | `cam_combined_nc.rs` convierte y reúne operaciones activas vigentes, genera un único NC y lo interpreta; solicitudes T/M6 explícitas | No hay simulación conjunta de material multiherramienta ni validación física de M6; Start/End personalizados se rechazan en conjunto |
| Planificador neutral | `cam_plan.rs` planifica enlaces entre operaciones con subir–XY–bajar a máximo de cotas de sus extremos; `plan_single` no inventa despeje de lote | No evalúa fijaciones ni volumen barrido ni transforma el contrato de posición física |
| Postprocesador | `nc_post.rs` usa **emisor común `emit_plan`** para individual y conjunto; conserva interfaces públicas, diferencias de admisión, T/M6, F/S, unidades, precisión, numeración, modal y metadatos | No es un perfil físico FluidNC validado; solicitudes M6 no garantizan un cambio real |
| Intérprete NC | Reconstruye el G-code final en modos soportados; registra los eventos T/M6 | Rechaza G53/G54 y otras instrucciones no implementadas; no simula acciones físicas del controlador |
| Informes y reproducción | Métricas sobre G0/G1 interpretados, avisos preliminares, vista compacta y sesión ligada a geometría y herramienta | Tiempos sin aceleraciones, estados iniciales teóricos y sin cobertura de movimientos de máquina externos |
| Simulación de material | Heightfield 2,5D plano/esférico, cambios acotados, rebobinado y OpenGL | Sin socavados, portaherramientas, colisiones completas o simulación multiherramienta del NC conjunto |
| Verificación | Envolvente de bruto original, posibles cruces G0 y estado de husillo | No acredita seguridad física ni material remanente real |
| UI de exportación | Interfaces individuales y conjuntas funcionales | Persiste la casilla de reconocimiento de riesgos en exportación: **no cumple todavía C8/C9**, su retirada es una tarea de UI pendiente |

## 3. Evolución documentada del flujo NC

- `85dfda2` (2026-11-18): cálculo secuencial de operaciones activas; **en ese corte histórico aún no existía NC conjunto**. No aplicar esa limitación al estado actual.
- Cambios posteriores hasta `6325e521`: se incorporaron generación de NC conjunto, solicitudes T/M6 opcionales y `cam_plan.rs`; `6325e521` extrajo los enlaces geométricos de `nc_post.rs`, preservando el NC y agregando pruebas golden. **373 pruebas reportadas** en Windows para ese corte, sin atribuirlas al actual.
- `b5b15248` (2026-11-19): `plan_single`, representación prestada de operaciones y `emit_plan` común; permanecen `generate_program` y `generate_batch_program` con políticas distintas. **379 pruebas reportadas** en Windows para el árbol local correspondiente.
- D-048 formaliza los nueve contratos CAM/NC (`RESPONSABILIDADES_CAM_NC.md`). Su aceptación **no demuestra por sí sola** que todo el código cumpla las responsabilidades, en particular UI/operador.

## 4. Contratos de estrategia, planificación, máquina y operador

- **D-045:** cada estrategia empieza en su primer XY a Z de seguridad definida sobre el bruto y conserva los enlaces internos entre pasadas. No busca aparcamiento/home ni posicionamiento de máquina.
- **D-046:** el operador responde de posición física inicial, homing, fijaciones, herramienta y origen; un post futuro podrá preparar retirada Z con G53 **solo si se configura y comprueba**. G53 aún no es interpretable en LaguCAM.
- **D-048 (incluye principio de D-047):** el planificador coordina enlaces entre operaciones y solicitudes neutrales, el post emite el NC, el intérprete reconstruye el texto final, verificador y simulador consumen esa traza. C8/C9 prohíben casillas de reconocimiento de riesgos para generar/exportar; el código actual conserva esas casillas como deuda de implementación.

## 5. Riesgos y trabajo abierto

1. **Seguridad CNC:** no hay verificación integral del volumen barrido, colisiones de portaherramientas/fijaciones, límites físicos o herramienta montada. El NC sigue siendo **experimental**; las pruebas Rust no autorizan mecanizado.
2. **Post y máquina:** G53 opcional, homing, referencias de máquina, perfil FluidNC y comportamiento de macros M6 requieren definición y comprobación específicas.
3. **Simulación conjunta:** existe generación e interpretación del NC conjunto, pero no simulación de eliminación de material conjunta multiherramienta ni demostración de compatibilidad de cambios físicos de herramienta.
4. **UI/UX:** persisten casillas y avisos repetidos contrarios al contrato aceptado de incidencias en contexto (C8/C9); resolver por cambio separado y aceptado.
5. **Regresión de integración:** faltan pruebas GUI con activación/desactivación, reordenación, cancelación, cambios de proyecto, regeneración y exportación de varias operaciones.
6. **Rendimiento:** sin benchmark del motor CAM/simulación para STL de ~1 GB; el rendimiento del visor comunicado históricamente no acredita el resto de la aplicación.
7. **Mantenimiento:** vigilar `preview_matrix()` y nomenclatura histórica; no refactorizar por longitud de archivos sin defecto demostrado (D-044).

**Siguiente decisión:** el usuario seleccionará el próximo hito. No hay autorización implícita para modificar UI, G53 o simulación. Consultar `DECISIONS.md`, `RESPONSABILIDADES_CAM_NC.md`, `WORKFLOW.md` y `HANDOFF.md`. **No se declaran cerrados P1–P4 completos.**

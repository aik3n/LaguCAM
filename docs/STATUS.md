# Estado verificable de LaguCAM

<!-- LAGUCAM-MASTER-DOC-CUT: 6d496200f5bf6ec70bb0bb8249543b315d3e1854 -->

**Corte documental:** 2026-11-23. **Código público inspeccionado:** [`6d496200f5bf6ec70bb0bb8249543b315d3e1854`](https://github.com/aik3n/LaguCAM/commit/6d496200f5bf6ec70bb0bb8249543b315d3e1854). Inspección estática del commit y comparación con documentación anterior; el árbol local de Windows no se ha verificado mediante un SHA independiente. Esta revisión documental no ejecuta Cargo, la GUI ni una CNC.

## 1. Evidencia del corte

| Evidencia | Resultado y procedencia |
| --- | --- |
| Código | Inspección estática del commit `6d496200` y sus cambios sobre `ef7d142b` |
| Pruebas automatizadas | **382/382 superadas (129 lib + 214 bin + 39 integración)**, según registros de PowerShell compartidos por el usuario durante este hito; los registros no acreditan por sí mismos identidad exacta del árbol local con el commit publicado |
| Formato y compilación | Ejecuciones previas de `cargo fmt` y `cargo build --release` comunicadas; después de la limpieza de UI apareció una regresión de avisos Clippy que motivó un correctivo; falta registro final de la cadena completa sobre el SHA exacto |
| Clippy estricto | **Pendiente de confirmación final** del correctivo posterior a la eliminación de diagnósticos (`-D warnings`); no tratar el registro fallido como aprobación |
| GUI manual | Usuario comprobó visibilidad independiente, cálculo, iconos Lucide y desaparición del bloqueo al desplegar operaciones; arrastre mediante icono y limpieza se han trabajado en el hito, pero no se documenta una batería gráfica completa |
| Persistencia | Prueba automatizada `operation_visibility_roundtrips_and_legacy_defaults_visible` incluida entre las aprobadas; round-trip manual integral por confirmar |
| Sin acreditar | Pruebas CNC, colisiones volumétricas, máquina FluidNC real, benchmark STL grande, simulación conjunta multiherramienta y regresión GUI exhaustiva |

Los números corresponden a la ejecución reportada, no a una validación nueva del Chat Master. Documentar no equivale a probar.

## 2. Capacidades implementadas inspeccionadas hasta `6d496200`

| Área | Estado presente en código | Limitación actual |
| --- | --- | --- |
| Proyecto y visor | STL externo, OpenGL, bruto, origen CNC, `.lagucam` v2, unidades y transformaciones no destructivas | Sin prueba integral de sincronización visor/BVH y rendimiento sobre STL grande en este corte |
| Biblioteca y geometría | Herramientas UUID, huella geométrica SHA-256 y ContactSolver plana/esférica/toroidal | No verificadas colisiones de conjunto herramienta, mango, portaherramientas y fijaciones |
| Acabado paralelo TEST | Contactos, sobreespesor, zigzag, aproximaciones, cortes, retiradas y enlaces internos neutrales | No es desbaste ni recorrido CNC certificado |
| Procedencia | `CamProvenance`, identidad y caducidad de resultados, firma económica del STL | La firma del STL no equivale a hash integral ni a validación de máquina |
| Operación individual | Calcular, generar NC, interpretar y reproducir el NC; Start/End admitidos por su contrato | La posición inicial requerida es una precondición teórica del programa, no un estado físico verificado |
| Cálculo de varias operaciones | `cam_batch.rs` coordina las **activas** en orden, reutilización, cancelación y reordenación; la visibilidad gráfica es independiente | Faltan pruebas manuales integrales de ediciones concurrentes y cancelación |
| NC conjunto | `cam_combined_nc.rs` convierte y reúne operaciones activas vigentes, genera un único NC y lo interpreta; solicitudes T/M6 explícitas | No hay simulación conjunta de material multiherramienta ni validación física de M6; Start/End personalizados se rechazan en conjunto |
| Planificador neutral | `cam_plan.rs` planifica enlaces entre operaciones con subir–XY–bajar a máximo de cotas de sus extremos; `plan_single` no inventa despeje de lote | No evalúa fijaciones ni volumen barrido ni transforma el contrato de posición física |
| Postprocesador | `nc_post.rs` usa **emisor común `emit_plan`** para individual y conjunto; conserva interfaces públicas, diferencias de admisión, T/M6, F/S, unidades, precisión, numeración, modal y metadatos | No es un perfil físico FluidNC validado; solicitudes M6 no garantizan un cambio real |
| Intérprete NC | Reconstruye el G-code final en modos soportados; registra los eventos T/M6 | Rechaza G53/G54 y otras instrucciones no implementadas; no simula acciones físicas del controlador |
| Informes y reproducción | Métricas sobre G0/G1 interpretados, avisos preliminares, vista compacta y sesión ligada a geometría y herramienta | Tiempos sin aceleraciones, estados iniciales teóricos y sin cobertura de movimientos de máquina externos |
| Simulación de material | Heightfield 2,5D plano/esférico, cambios acotados, rebobinado y OpenGL | Sin socavados, portaherramientas, colisiones completas o simulación multiherramienta del NC conjunto |
| Verificación | Envolvente de bruto original, posibles cruces G0 y estado de husillo | No acredita seguridad física ni material remanente real |
| UI de exportación | Interfaces individuales y conjuntas; el usuario ha comunicado retirada de casillas redundantes y avisos no bloqueantes en cambios anteriores | Confirmar regresión final de C8/C9 en todos los caminos; solo los errores técnicos deben bloquear la exportación |

## 3. Evolución documentada del flujo NC

### UI y persistencia de operaciones en `6d496200`

- Cada operación tiene `active` (participa en el cálculo/NC conjunto) y `visible` (se dibuja en 3D); ambos estados son independientes. Cambiar la visibilidad no debe invalidar el resultado CAM.
- El campo `visible` se guarda en `.lagucam`; un documento anterior sin ese campo se interpreta como visible. La serialización no implica persistencia de resultados CAM calculados.
- Iconos de visibilidad **Lucide**: ojo verde / ojo tachado gris, recursos RGBA integrados con licencia ISC. El icono de operación actúa como asa de reordenación; pulsar el nombre selecciona y el doble clic edita.
- Editor y menú `Vista` se han simplificado: se quitaron botones duplicados de cálculo en el editor y diagnósticos geométricos experimentales del menú. Los módulos de diagnóstico permanecen en código para conservar contratos/pruebas; no son funciones expuestas en el menú.
- La actualización del diseño y redimensionamiento automático del editor queda aplazada. La eliminación de controles no equivale a una redistribución de la UI.


- `85dfda2` (2026-11-18): cálculo secuencial de operaciones activas; **en ese corte histórico aún no existía NC conjunto**. No aplicar esa limitación al estado actual.
- Cambios posteriores hasta `6325e521`: se incorporaron generación de NC conjunto, solicitudes T/M6 opcionales y `cam_plan.rs`; `6325e521` extrajo los enlaces geométricos de `nc_post.rs`, preservando el NC y agregando pruebas golden. **373 pruebas reportadas** en Windows para ese corte, sin atribuirlas al actual.
- `b5b15248` (2026-11-19): `plan_single`, representación prestada de operaciones y `emit_plan` común; permanecen `generate_program` y `generate_batch_program` con políticas distintas. **379 pruebas reportadas** en Windows para el árbol local correspondiente.
- D-048 formaliza los nueve contratos CAM/NC (`RESPONSABILIDADES_CAM_NC.md`). Su aceptación **no demuestra por sí sola** que todo el código cumpla las responsabilidades, en particular UI/operador.

## 4. Contratos de estrategia, planificación, máquina y operador

- **D-045:** cada estrategia empieza en su primer XY a Z de seguridad definida sobre el bruto y conserva los enlaces internos entre pasadas. No busca aparcamiento/home ni posicionamiento de máquina.
- **D-046:** el operador responde de posición física inicial, homing, fijaciones, herramienta y origen; un post futuro podrá preparar retirada Z con G53 **solo si se configura y comprueba**. G53 aún no es interpretable en LaguCAM.
- **D-048 (incluye principio de D-047):** el planificador coordina enlaces neutrales entre operaciones; el post emite NC y el intérprete reconstruye el texto final para verificación/reproducción. C8/C9 rechazan confirmaciones de riesgo redundantes; los cambios de UI reportados avanzan ese criterio, pero falta verificación de todas las rutas.

## 5. Riesgos y trabajo abierto

1. **Seguridad CNC:** no hay verificación integral del volumen barrido, colisiones de portaherramientas/fijaciones, límites físicos o herramienta montada. El NC sigue siendo **experimental**; las pruebas Rust no autorizan mecanizado.
2. **Post y máquina:** G53 opcional, homing, referencias de máquina, perfil FluidNC y comportamiento de macros M6 requieren definición y comprobación específicas.
3. **Simulación conjunta:** existe generación e interpretación del NC conjunto, pero no simulación de eliminación de material conjunta multiherramienta ni demostración de compatibilidad de cambios físicos de herramienta.
4. **UI/UX:** verificar C8/C9 en todas las rutas de exportación y persistencia/visibilidad/reordenamiento tras la limpieza. Ajustar tamaño y distribución del editor en un hito posterior.
5. **Regresión de integración:** faltan pruebas GUI con activación/desactivación, reordenación, cancelación, cambios de proyecto, regeneración y exportación de varias operaciones.
6. **Rendimiento:** sin benchmark del motor CAM/simulación para STL de ~1 GB; el rendimiento del visor comunicado históricamente no acredita el resto de la aplicación.
7. **Mantenimiento:** vigilar `preview_matrix()` y nomenclatura histórica; no refactorizar por longitud de archivos sin defecto demostrado (D-044).

**Siguiente decisión:** el usuario seleccionará el próximo hito. No hay autorización implícita para modificar UI, G53 o simulación. Consultar `DECISIONS.md`, `RESPONSABILIDADES_CAM_NC.md`, `WORKFLOW.md` y `HANDOFF.md`. **No se declaran cerrados P1–P4 completos.**

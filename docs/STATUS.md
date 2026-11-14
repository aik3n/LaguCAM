# Estado actual de LaguCAM

**Fecha del corte:** 2026-11-14  
**Rama examinada:** `main`  
**SHA:** `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9` — «puntos de origen»  
**Método de comprobación:** lectura del repositorio y archivos; experiencias funcionales comunicadas por el usuario. **No se ha compilado ni ejecutado aquí el programa gráfico.**

## 1. Estado breve

**Hito del visor 3D grande: probado por el usuario.** Carga, visualización y navegación con un STL real de casi 1 GB: **USUARIO-VALIDADO**. No están publicados valores medidos de tiempo de carga, memoria, VRAM o FPS, ni una matriz multiplataforma.

El árbol lateral, bruto y origen constituyen una base de preparación CAM; **no existe motor de trayectorias en esta revisión**.

## 2. Inventario de capacidades

| Capacidad | Evidencia | Nivel |
| --- | --- | --- |
| Lectura STL binario estricto, dos pasadas, cancelación | `src/stl.rs` | CÓDIGO-VERIFICADO |
| Chunk de 65.536 triángulos, canal acotado, VBO GPU | `src/preview.rs`, `src/render.rs` | CÓDIGO-VERIFICADO |
| Apertura y navegación STL casi 1 GB | Informe del usuario | USUARIO-VALIDADO |
| Cámara rotación/zoom/pan/centrar | `src/main.rs` y prueba del usuario | CÓDIGO-VERIFICADO + USUARIO-VALIDADO |
| Árbol proyecto y selección de componentes | `src/project_ui.rs` | CÓDIGO-VERIFICADO |
| Bruto rectangular, extras seis lados, colores y opacidad | `src/project/stock.rs`, `display.rs`, `stock_render.rs` | CÓDIGO-VERIFICADO; funcionamiento general confirmado por usuario |
| Origen relativo a modelo o bruto, anclas y offset | `src/project/origin.rs`, `overlay.rs` | CÓDIGO-VERIFICADO |
| Rotación matemática de ejes CNC Rz·Ry·Rx | `src/project/origin.rs` | CÓDIGO-VERIFICADO; **sin post CNC asociado** |
| Color configurable del modelo | `src/project_ui.rs`, `src/render.rs` | CÓDIGO-VERIFICADO |
| Colección ordenada de operaciones | `src/project/operations.rs` | ESTRUCTURA CÓDIGO, **sin operaciones CAM** |
| Guardar/abrir proyectos | `src/project/persistence.rs`, `src/main.rs` | CÓDIGO-VERIFICADO en HEAD, validación gráfica de usuario pendiente |
| Biblioteca global de fresas y editor | `src/tools/*`, `src/tools_ui.rs`, `src/tools_profile.rs` | IMPLEMENTACIÓN PROPUESTA en este cambio, pendiente de Cargo y validación del usuario |
| Perfiles de máquina / postprocesadores | No encontrado en código | PENDIENTE |
| Cálculo de trayectorias / G-code / simulación | No encontrado en código | PENDIENTE |
| Abrir STEP | No encontrado | FUTURO |

## 3. Limitaciones constatadas

- La lectura actual solo admite STL **binario**, no ASCII.
- El render GPU conserva todas las caras; no hay decimación/LOD, culling espacial ni garantías generales de VRAM.
- La visualización `f32` no es un backend geométrico de alta exactitud para CAM.
- No hay prueba de colisiones de herramienta, husillo, mango, portaherramientas o sujeción.
- Los puntos de origen CNC y sus ejes se representan en overlay 2D, sin oclusión física completa.
- Las rotaciones visuales del origen no equivalen a cinemática real o postprocesado verificado.
- Los proyectos `.lagucam` se gestionan con un esquema versionado. La biblioteca es global e independiente de esos archivos.
- `README.md`, `AUDITORIA.md` y `PROJECT_TREE.md` reflejan en partes versiones anteriores. En particular, el README todavía describe un límite provisional que ya no figura en el flujo por bloques actual y menciona CI no presente en el árbol consultado.
- El paquete Cargo sigue llamándose `cam-libre-visor`, pese a llamarse LaguCAM el repositorio y la ventana.
- No se observa `.github/workflows` en el árbol `main` consultado; no se debe afirmar que exista CI activo.

## 4. Pruebas y confianza

**Confirmado por el usuario:** abrió y manejó correctamente una malla cercana a 1 GB; realizó comprobaciones del bruto/entorno visual y reportó funcionamiento correcto.

**Declarado en documentos previos:** la auditoría inicial refiere compilación y pruebas Rust en Linux, y los documentos de entrega refieren comandos `fmt`, `clippy`, `test` y `build` ejecutados mediante instaladores o previstos localmente. **Eso no acredita automáticamente que el SHA actual haya sido compilado/ensayado en todos los sistemas.** `LAGUCAM_006.md` indica expresamente que la compilación y OpenGL reales debían comprobarse en Windows.

**No documentado cuantitativamente:** carga de 1 GB con cronómetro, máximo RSS, VRAM, FPS, latencia de cancelación, validez de profundidad GL en distintos controladores y funcionamiento en otros equipos.

## 5. Siguiente hito

**Persistencia `.lagucam` observada en HEAD** (`src/project/persistence.rs` y `src/main.rs`). Su aceptación exige la prueba de usuario de cierre y reapertura; no se confunde el código con una validación gráfica.

A continuación: modelo y biblioteca de herramientas, empezando por acordar geometrías y parámetros antes de generar una tarea de implementación.

## 6. Historial de cambios de estado

- **2026-11-14 — línea base inicial.** Foto del commit `0e56d3f3`; experiencia positiva de STL grande registrada; deuda documental identificada.
- **Por completar en el siguiente commit:** nuevo SHA, qué cambió, pruebas realizadas, validación de usuario y efectos en el roadmap.

## Biblioteca global de herramientas (entrega pendiente de validación)

**Base de código examinada:** `1e5d5868547fb49e7d151e3d4402d9edfdd1c87f`. `HEAD` contiene persistencia `.lagucam` desde `6eb12e1`, pese al corte documental anterior.

**Implementación de esta entrega:** modelos de cinco fresas en `src/tools/model.rs`, guardado global versionado en `src/tools/storage.rs`, editor y filtros `src/tools_ui.rs`, perfil técnico `src/tools_profile.rs`. No hay asignación de T, operaciones CAM ni modificación de `.lagucam`.

**Pruebas del cambio:** el instalador comprueba `fmt`, `clippy`, `test` y `build --release`; resultado pendiente de la ejecución real del usuario. **Prueba gráfica de herramientas y regresión STL cercano a 1 GB: pendiente.**

## Ajuste 009 — Estado y evidencia

- **Preparado para probar en Windows:** la biblioteca de herramientas se abre solo desde el menú principal; no aparece en el árbol del proyecto. Su almacenamiento global no cambia y no requiere STL abierto.
- **Preparado para probar en Windows:** el fondo del visor 3D usa degradado azul cielo; STL y bruto conservan el render por profundidad. Es una modificación visual; no se ha medido rendimiento con STL de casi 1 GB.
- **Diseño acordado / no implementado aún:** cada operación futura referenciará el UUID global de una fresa; no habrá colección de fresas del proyecto ni copias ocultas. La operación fallará ante herramienta ausente; cambios invalidarán resultados anteriores.
- **Evidencia:** lectura del HEAD `1e5d5868547fb49e7d151e3d4402d9edfdd1c87f` y preparación del instalador 009; `fmt`, `clippy`, `test`, `release` y prueba gráfica pendientes de ejecución en Windows.

## Implementación 010 — Pivote visual

- **CÓDIGO PREPARADO, PENDIENTE DE COMPILAR Y PROBAR EN WINDOWS:** doble clic sobre la superficie visible para fijar el pivote orbital. Marca cruzada en pantalla; `Vista > Centrar vista` recupera centro y zoom inicial.
- Arquitectura independiente del formato. Nueva implementación de picking sobre la profundidad que ya existe en GPU; no se añade memoria proporcional al número de triángulos ni se vuelve a leer el archivo STL.
- Un fallo de driver o clic en el cielo no modifica el pivote. Persistencia `.lagucam`, herramienta global, bruto, origen CNC, formato de STL y operaciones no cambian.
- **Validaciones pendientes:** `cargo fmt --check`, `cargo clippy`, `cargo test`, `cargo build --release`, Windows/OpenGL y prueba del STL grande. No se atribuyen resultados no ejecutados.
- Base de esta modificación: `46f66fce07236e296a0c92dc27645b3b2e3e6cf8`; entrega sin commit nuevo.

## Ajuste 011 — Interaccion de pivote

- Cambio solicitado y aprobado: un clic simple sobre el modelo elige el pivote SIN mover la vista. Girar posteriormente orbita alrededor de ese punto. Un indicador marca su posicion sobre la geometria y «Vista > Centrar vista» lo restablece.
- El usuario confirmo que la 010 v2 compila y arranca en Windows; este ajuste 011 y el comportamiento grafico siguen pendientes de prueba funcional.
- `cargo fmt`, `clippy`, `test` y `build --release` se ejecutaran en el instalador del usuario; no se atribuyen resultados antes de su ejecucion.

## Ajuste 012 — Pivote al presionar el raton

- El responsable confirmo funcionamiento de LaguCAM 011, pero solicito poder pulsar y arrastrar en un mismo gesto y mantener visible la marca al soltar.
- Implementacion preparada en `src/main.rs`: `mouse down` fija el pivote mediante consulta GL; el arrastre se pospone hasta disponer del punto. No se alteran los datos ni el origen CNC.
- La compilacion Rust, la prueba con DPI/MSAA y la regresion con STL grande estan pendientes de ejecucion en Windows. No se asumen superadas en este documento.

## Ajuste 013 — Indicador transitorio del pivote

- El responsable confirma que LaguCAM 012 v2 compila y funciona; solicita que el indicador solo sea visible mientras se mantiene la pulsacion para rotar.
- Implementacion 013: el pivote sigue seleccionado al soltar, pero la marca se oculta. La asignacion de botones queda centralizada y preparada para una futura configuracion.
- Compilacion, Clippy, tests, release y prueba grafica de 013 pendientes de ejecucion en Windows; sin commit automatico.

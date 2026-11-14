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
| Guardar/abrir proyectos | No encontrado en árbol actual | PENDIENTE |
| Biblioteca global de fresas y editor | No encontrado en código | PENDIENTE |
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
- El estado del proyecto se reinicia al cargar un modelo nuevo; no se ha implementado persistencia.
- `README.md`, `AUDITORIA.md` y `PROJECT_TREE.md` reflejan en partes versiones anteriores. En particular, el README todavía describe un límite provisional que ya no figura en el flujo por bloques actual y menciona CI no presente en el árbol consultado.
- El paquete Cargo sigue llamándose `cam-libre-visor`, pese a llamarse LaguCAM el repositorio y la ventana.
- No se observa `.github/workflows` en el árbol `main` consultado; no se debe afirmar que exista CI activo.

## 4. Pruebas y confianza

**Confirmado por el usuario:** abrió y manejó correctamente una malla cercana a 1 GB; realizó comprobaciones del bruto/entorno visual y reportó funcionamiento correcto.

**Declarado en documentos previos:** la auditoría inicial refiere compilación y pruebas Rust en Linux, y los documentos de entrega refieren comandos `fmt`, `clippy`, `test` y `build` ejecutados mediante instaladores o previstos localmente. **Eso no acredita automáticamente que el SHA actual haya sido compilado/ensayado en todos los sistemas.** `LAGUCAM_006.md` indica expresamente que la compilación y OpenGL reales debían comprobarse en Windows.

**No documentado cuantitativamente:** carga de 1 GB con cronómetro, máximo RSS, VRAM, FPS, latencia de cancelación, validez de profundidad GL en distintos controladores y funcionamiento en otros equipos.

## 5. Siguiente hito

**Etapa 1: persistencia de proyectos.** La tarea ya tiene especificación conversada, pero **no se ha identificado implementación en `main`**. No darla por cerrada hasta verificar código, tests y una sesión de usuario «abrir STL → configurar → guardar → cerrar → abrir proyecto → recuperar valores».

A continuación: modelo y biblioteca de herramientas, empezando por acordar geometrías y parámetros antes de generar una tarea de implementación.

## 6. Historial de cambios de estado

- **2026-11-14 — línea base inicial.** Foto del commit `0e56d3f3`; experiencia positiva de STL grande registrada; deuda documental identificada.
- **Por completar en el siguiente commit:** nuevo SHA, qué cambió, pruebas realizadas, validación de usuario y efectos en el roadmap.

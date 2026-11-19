# Arquitectura de LaguCAM

<!-- LAGUCAM-MASTER-DOC-CUT: 85dfda28658d5f8e779caae97838742d1444be73 -->

**Arquitectura contrastada con código público:** `85dfda2` (2026-11-18), por inspección estática. Pruebas y limitaciones en [`STATUS.md`](STATUS.md). La etiqueta [ACTUAL] indica presencia en código, **no** seguridad CNC ni validación en Windows.

## 1. Stack y distribución [ACTUAL]

Rust edición 2024, `rust-version = 1.100`; egui/eframe con render OpenGL/glow, y dependencias en `Cargo.toml`. El paquete Cargo aún se llama `cam-libre-visor`; la aplicación se denomina LaguCAM.

La distribución portable sitúa `settings.json`, `panels.json`, `tools.json` y `recent_projects.json` bajo `config/` junto al ejecutable (`portable.rs`). Los proyectos `.lagucam` son archivos externos independientes; se guardan mediante reemplazo, sin copia de recuperación automática.

## 2. Módulos [ACTUAL]

| Módulo | Responsabilidad |
| --- | --- |
| `stl.rs` | STL binario, dos pasadas, validación, límites `f64` y cancelación |
| `preview.rs` | Bloques visuales de 65.536 triángulos, transporte por canal acotado |
| `render.rs` | Render GPU, VBO, shaders y profundidad del STL |
| `stock_render.rs` | Render independiente del bruto translúcido y aristas |
| `overlay.rs` | Marcadores y ejes CNC superpuestos |
| `navigation.rs`, `nav_cube.rs`, `scene_pick.rs` | Cámara, cubo de orientación y picking sobre profundidad GPU; **no consultas CAM** |
| `project/*` | Modelo, bruto, origen, operaciones parametrizables y persistencia del proyecto |
| `project/persistence.rs` | Serialización `.lagucam` v2, validación y comprobación económica del STL externo |
| `project/model_transform.rs` | Origen mm/pulgadas, escala, rotación, traslación y pivote; transformación afín no destructiva compartida entre visor y CAM |
| `project/cam_provenance.rs` | Dependencias geométricas de previews TEST; invalidación por STL, bruto, estrategia, transformación y fresa |
| `project_ui.rs` | Árbol y propiedades; selección, activación, reordenación y comandos de operaciones |
| `tools/model.rs` | Definición y validación de seis familias de fresas, UUID y perfiles geométricos `f64` |
| `tools/storage.rs` | Biblioteca global versionada y persistencia independiente |
| `tools_ui.rs`, `tools_profile.rs` | Editor/filtros y perfil gráfico de las fresas |
| `panel_layout.rs` | Paneles flotantes/acoplables y distribución persistente |
| `portable.rs`, `options.rs`, `recent_projects.rs` | Configuración portable, temas y historial de proyectos recientes |
| `cam_geometry/{mod,stl,service,contact_solver,flat_allowance,bull}.rs` | Contacto esférico, plano y toroidal `f64`, BVH XY, sobrematerial euclídeo y acceso protegido a STL; **no certifica colisiones** |
| `cam_geometry/section.rs` | Extracción de secciones sobre triángulos originales, independiente del render |
| `test_parallel.rs` | Estrategia acabado paralelo TEST: muestreo, herramienta UUID, sobreespesor, patrón, cálculo asíncrono, visualización y validación estructural |
| `tools/cam_adapter.rs` | Resolución UUID → geometría `CutterGeometry`, huella SHA-256 e invalidación de previsualizaciones |
| `toolpath.rs`, `toolpath_links.rs`, `toolpath_validation.rs` | Trayectoria neutral de contacto, movimientos propuestos y **validador solo estructural** |
| `cam_contract.rs`, `project/cam_coordinates.rs` | Vista inmutable de operación/F/S y conversión al origen de trabajo con traslación y giro Rz |
| `cam_markers_3d.rs` | Indicadores visuales de principio y fin de recorrido, no geometría de corte |
| `sample_direct.rs`, `sample_overlay.rs`, `section_overlay.rs` | Instrumentos transitorios de diagnóstico geométrico |
| `cam_batch.rs` | Cola en memoria para cálculo secuencial de operaciones activas; no genera NC ni geometría |
| `nc_post.rs`, `post_ui.rs` | Postprocesador 3 ejes y biblioteca de perfiles, cabeceras Start/End; exportación experimental |
| `nc_contract.rs`, `nc_interpreter.rs` | Contrato modal de programa NC e intérprete estricto del texto final; G53 no soportado |
| `nc_export_ui.rs`, `nc_verify.rs` | Resultados del programa NC, métricas, alertas conservadoras de bruto original y exportación |
| `nc_playback_ui.rs`, `nc_tool_link.rs` | Reproducción, vigencia de sesión NC, correspondencia UUID/SHA-256 de herramienta y controles compactos |
| `material_sim.rs`, `material_ui.rs`, `nc_scene_gl.rs`, `nc_tool_overlay.rs` | Simulación aproximada 2,5D y render de material/herramienta; no colisiones integrales |
| `main.rs` | Coordinación de UI, trabajos CAM, cálculo por lote, interpretación/reproducción NC, render y diagnósticos |

`main.rs` y `panel_layout.rs` son extensos, pero no requieren refactorización por tamaño sin una necesidad concreta. El solver no depende del render. Los movimientos neutros G25 están separados de la estrategia, aunque **todavía no están verificados para postprocesado seguro**.

## 3. Geometría y render [ACTUAL]

```text
STL binario externo → validar y obtener límites f64 → triángulos por bloques
                    → canal acotado → VBO de OpenGL → render STL
                    → render del bruto con profundidad → overlays/UI
```

La GPU conserva las caras renderizadas, pero su geometría normalizada en `f32` **no es la representación geométrica de autoridad para CAM**. La lectura del depth buffer sirve solo para navegación visual. El STL fuente se conserva sin modificaciones; el importador no verifica que sea cerrado, manifold o apto para mecanizado.

El bruto es un prisma alineado con los ejes físicos de modelo, con límites y extras por cara `f64`. El STL sigue inmutable: `raw_bounds` guarda límites originales, `bounds` refleja el modelo transformado a milímetros, y `ModelTransform` define la matriz compartida. El renderer aplica la transformación a la malla normalizada y el BVH transforma cada triángulo al calcular. **Riesgo observado:** `main.rs` usa `Mat4::IDENTITY` si no puede crear la matriz visual; P1 debe impedir una divergencia visual/CAM silenciosa. El sistema de trabajo usa anclas en modelo/bruto, offsets y `R = Rz · Ry · Rx`, con `p_modelo = origen + R · p_CNC`. Una rotación matemática del marco no significa que `G54` rote ejes ni que una CNC de tres ejes pueda orientar el husillo.

`.lagucam` guarda referencia externa del STL y configuración de proyecto, no la malla ni una copia editable de la biblioteca global. Las operaciones usan `tool_id: Option<ToolId>`; el editor permite seleccionar la fresa y `cam_adapter.rs` la resuelve antes del cálculo. Las previews usan `CamProvenance` para comprobar identidad muestreada/fecha/tamaño de STL, AABB físico, configuración de transformación, bruto, estrategia y firma UUID/hash de fresa. **No es una firma criptográfica del STL completo ni un contrato de exportación:** setup/origen y parámetros F/S requieren su propia validación previa al post. Los resultados de cálculo permanecen fuera del proyecto persistido.

## 4. Modelo de fresas [ACTUAL]

`tools::model::ToolId` es un alias de `String` con formato UUID, validado por `validate_tool_id`. Biblioteca, `Operation` y `SavedOperation` comparten este alias. La biblioteca asigna UUIDv4 y aumenta revisión al editar.

`ToolGeometry` describe fresa plana, esférica, toroidal (*bull nose*), V, cónica de punta plana y cónica de punta esférica. Sus parámetros son `f64`. El método `profile()` dibuja la **representación 2D**; solo esférica, plana y toroidal tienen solución de contacto CAM en el código actual.

La biblioteca es la **única fuente editable**. Ninguna operación conserva una copia editable ni parámetros ocultos de fresa; no hay número T intrínseco.

## 5. Huella geométrica y validez parcial de resultados [ACTUAL]

`src/tools/hash.rs` implementa SHA-256 sobre representación **binaria canónica y versionada** de los parámetros geométricos efectivos: variante, diámetros, radios, ángulos, longitud de corte, diámetro de mango y longitud total, ampliables con otras cotas físicas que influyan en contacto o colisión. Excluir UUID, revisión, nombre, notas, RPM y avances recomendados. Normalizar valores equivalentes y rechazar no finitos.

El cálculo resuelve la fresa por UUID y conserva una firma (UUID + SHA-256). Desde P1-A, `CamProvenance` invalida previews por fuente, bruto, modelo transformado, herramienta y parámetros geométricos; no almacena G-code. Aún falta definir y comprobar la **validez de exportación**: origen/setup de trabajo, tolerancias, verificación geométrica, máquina y post. Ø6 → Ø4 → Ø6 puede restaurar el hash anterior, pero **no** acredita la trayectoria completa ni ningún G-code.

La huella UUID/SHA y la procedencia se usan para vigencia de **previsualizaciones geométricas**. Desde `d108dfad`, la reproduccion NC conserva una identidad de sesion (fuente STL, transformacion, bruto, version del NC) y comprueba herramienta frente a la geometria de origen. Siguen pendientes la validez fisica de maquina y la verificacion integral de barrido/colisiones. Hay trayectoria neutral y validador estructural; **no** hay trayectoria certificada para CNC. El contrato v1 utiliza el dominio `LaguCAM/tool-geometry/SHA-256\0`, versión `u32 LE=1`, discriminantes fijos `u8` (plana=1, esférica=2, V=3, cónica plana=4, cónica esférica=5, toroidal=6), las dimensiones específicas de cada variante en el orden de su declaración, y finalmente longitud de corte, diámetro de mango y longitud total. Los `f64` se escriben mediante IEEE-754 little-endian, normalizando el cero con signo y rechazando geometría inválida. Un cambio del contrato requiere nueva versión y vectores de prueba.

El contacto esférico con STL se calcula a partir de los valores `f32` originales convertidos a `f64`, nunca desde la normalización gráfica. El BVH usa límites XY y posiciones de registro para no guardar toda la malla dos veces; su presupuesto no incluye toda la RSS del proceso. La fachada del servicio de contacto tiene protección de inmutabilidad de archivo implementada en Windows; otras plataformas siguen pendientes.

**Precaución:** cada punto muestreado puede ser un contacto válido sin que el **movimiento entre puntos** lo sea. El muestreo TEST no verifica el volumen barrido ni colisiones de mango y portaherramientas.

## 6. Flujo de CAM, post, interpretación y simulación [CÓDIGO OBSERVADO EN 85dfda2]

```text
Proyecto STL + modelo transformado + bruto + origen + herramienta UUID
   |  -> estrategia acabado paralelo TEST / ContactSolver
   v
Trayectoria neutral y enlaces internos -> validación ESTRUCTURAL
   |   primer punto = XY inicial de operacion a Z de despeje de estrategia
   v
WorkFrame modelo -> trabajo (mm) -> nc_post.rs -> G-code FINAL
   |                                  -> nc_export_ui: métricas y alertas
   v
nc_interpreter.rs (texto NC final, modos soportados) -> NcTrace
   |                                        |
   +-> nc_verify.rs: bruto original          +-> material_sim.rs: heightfield 2,5D
   |                                        |
   +-> avisos enlazados al visor             +-> nc_playback_ui/nc_scene_gl: reproducir

Operaciones activas -> cam_batch.rs -> secuencia de calculos CAM independientes
    [NO concatena NC; NO planifica transiciones de maquina entre operaciones]
```

- **Frontera CAM:** `toolpath_links.rs` empieza en el primer XY real de mecanizado a la Z de clearance calculada con el bruto/geometria; conserva G0 XY de enlace entre pasadas **dentro** de la operacion. No inventa el movimiento desde la posicion fisica de maquina. Acuerdo normativo D-045.
- **Frontera de post y maquina:** el post traduce la trayectoria y puede, cuando se implemente y valide el perfil, incorporar una retirada Z de maquina mediante G53 y movimiento XY al inicio; el operador es responsable de condiciones fisicas. No convertir `G53` en una funcion de estrategia. Acuerdo D-046.
- **Frontera de operaciones multiples:** la cola `cam_batch.rs` administra cálculo de varias operaciones, no un planificador de recorridos entre operaciones. Un futuro planificador es PROPUESTA D-047.
- **Frontera de verificacion:** el intérprete rechaza G53, G54, arcos y otros codigos no soportados; los Start/End personalizados pueden impedir metricas/simulacion automatica. `nc_verify.rs` solo informa intersecciones candidatas respecto al bruto original; no verifica el material restante ni el portaherramientas.

El motor inicial sigue limitado a tres ejes y herramienta paralela a Z del setup. Máquina, estrategia, planificador y post deben tener contratos independientes; no asumir `M6` ni rotacion por `G54`. Las geometrías futuras se conectarán por capacidades del backend, sin imponer triangulación universal.

## 7. Riesgos e invariantes

1. UUID con formato válido no equivale a herramienta existente; `cam_adapter` debe resolverlo o rechazar la operación.
2. `CamProvenance` cubre procedencia geométrica para previews; **no equivale a verificación integral ni contrato exportable**. Su firma muestreada STL no verifica criptográficamente cada byte.
3. El mango actual no determina salida de pinza; faltan cuello, portaherramientas, sujeciones y comprobación de colisión.
4. `f32` normalizado y depth buffer son visuales, no precisión CAM.
5. `.lagucam` v1 se rechaza: **durante desarrollo no se exige compatibilidad**, pero se documentan roturas y no se borran datos personales silenciosamente.
6. La compilación no demuestra seguridad CNC; toda salida futura requerirá revisión y pruebas controladas.

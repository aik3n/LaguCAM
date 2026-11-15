# Arquitectura de LaguCAM

**Arquitectura contrastada con:** `13a439a` (2026-11-15). El estado de implementación y sus pruebas se consulta **únicamente** en [`STATUS.md`](STATUS.md); aquí se documentan contratos y límites. Las etiquetas [ACTUAL] no implican validación CNC.

## 1. Stack y distribución [ACTUAL]

Rust edición 2024, `rust-version = 1.100`; egui/eframe con render OpenGL/glow, y dependencias en `Cargo.toml`. El paquete Cargo aún se llama `cam-libre-visor`; la aplicación se denomina LaguCAM.

Se diseñó una distribución portable: `portable.rs` sitúa `settings.json`, `panels.json` y `tools.json` bajo `config/` junto al ejecutable; los proyectos `.lagucam` son archivos externos independientes.

## 2. Módulos [ACTUAL]

| Módulo | Responsabilidad |
| --- | --- |
| `stl.rs` | STL binario, dos pasadas, validación, límites `f64` y cancelación |
| `preview.rs` | Bloques visuales de 65.536 triángulos, transporte por canal acotado |
| `render.rs` | Render GPU, VBO, shaders y profundidad del STL |
| `stock_render.rs` | Render independiente del bruto translúcido y aristas |
| `overlay.rs` | Marcadores y ejes CNC superpuestos |
| `navigation.rs`, `scene_pick.rs` | Cámara y selección visual sobre profundidad GPU; **no consultas CAM** |
| `project/*` | Modelo, bruto, origen, operaciones parametrizables y persistencia del proyecto |
| `project/persistence.rs` | Serialización `.lagucam` v2, validación y comprobación de STL externo |
| `project_ui.rs` | Árbol lateral y propiedades |
| `tools/model.rs` | Definición y validación de cinco tipos de fresas, UUID y perfiles geométricos `f64` |
| `tools/storage.rs` | Biblioteca global versionada y persistencia independiente |
| `tools_ui.rs`, `tools_profile.rs` | Editor/filtros y perfil gráfico de las fresas |
| `panel_layout.rs` | Paneles flotantes/acoplables y distribución persistente |
| `portable.rs`, `options.rs` | Configuración local y tema claro/oscuro |
| `cam_geometry/mod.rs`, `stl.rs`, `service.rs` | Contacto esférico `f64`, BVH XY sobre triángulos STL, lectura con caché y fachada del motor; **no certifica ausencia de colisiones** |
| `cam_geometry/section.rs` | Extracción de secciones sobre triángulos originales, independiente del render |
| `test_parallel.rs` | Estrategia experimental de muestreo de contactos, cancelación y previsualización; no es una trayectoria CNC |
| `sample_direct.rs`, `sample_overlay.rs`, `section_overlay.rs` | Instrumentos transitorios de diagnóstico geométrico |
| `main.rs` | Coordinación de UI, flujo de archivos, importación en segundo plano, render y diagnósticos |

`main.rs` y `panel_layout.rs` son extensos, pero no conviene refactorizarlos solo por longitud. Las consultas CAM residen en `cam_geometry/*` y no dependen del render. Los experimentos visuales de `test_parallel.rs` no son un motor de estrategia listo para postprocesar.

## 3. Geometría y render [ACTUAL]

```text
STL binario externo → validar y obtener límites f64 → triángulos por bloques
                    → canal acotado → VBO de OpenGL → render STL
                    → render del bruto con profundidad → overlays/UI
```

La GPU conserva las caras renderizadas, pero su geometría normalizada en `f32` **no es la representación geométrica de autoridad para CAM**. La lectura del depth buffer sirve solo para navegación visual. El STL fuente se conserva sin modificaciones; el importador no verifica que sea cerrado, manifold o apto para mecanizado.

El bruto es un prisma alineado con los ejes de modelo, con límites y extras por cara `f64`. El sistema de trabajo usa anclas en modelo/bruto, offsets y `R = Rz · Ry · Rx`, con `p_modelo = origen + R · p_CNC`. Una rotación matemática del marco no significa que `G54` rote ejes ni que una CNC de tres ejes pueda orientar el husillo.

`.lagucam` v2 guarda el proyecto y referencia externa del STL, no la malla ni la biblioteca global. Las operaciones pueden conservar parámetros de acabado paralelo TEST y `tool_id: Option<ToolId>`; **no existe todavía selector operativo ni resolución de herramienta de biblioteca para el cálculo**. Los resultados de muestra se mantienen fuera del proyecto persistido.

## 4. Modelo de fresas [ACTUAL]

`tools::model::ToolId` es un alias de `String` con formato UUID, validado por `validate_tool_id`. Biblioteca, `Operation` y `SavedOperation` comparten este alias. La biblioteca asigna UUIDv4 y aumenta revisión al editar.

`ToolGeometry` describe fresa plana, esférica, V, cónica de punta plana y cónica de punta esférica. Sus parámetros están en `f64`. El método `profile()` produce segmentos para **representación 2D**, no una superficie exacta que deba reutilizarse para contacto CNC.

La biblioteca es la **única fuente editable**. Ninguna operación conserva una copia editable ni parámetros ocultos de fresa; no hay número T intrínseco.

## 5. Huella geométrica [IMPLEMENTADA] y validez de resultados [PENDIENTE]

`src/tools/hash.rs` implementa SHA-256 sobre representación **binaria canónica y versionada** de los parámetros geométricos efectivos: variante, diámetros, radios, ángulos, longitud de corte, diámetro de mango y longitud total, ampliables con otras cotas físicas que influyan en contacto o colisión. Excluir UUID, revisión, nombre, notas, RPM y avances recomendados. Normalizar valores equivalentes y rechazar no finitos.

En un futuro cálculo se tomará una **copia temporal inmutable** de la definición de biblioteca para impedir cambios durante el cálculo. La trayectoria registrará UUID, hash geométrico y dependencias (modelo, bruto, setup, estrategia, versión de algoritmo). Se bloqueará exportación de resultados incompatibles. Ø6 → Ø4 → Ø6 puede restaurar un hash de herramienta idéntico, **pero eso no demuestra por sí solo vigencia de la trayectoria ni del G-code**.

El hash funciona como **función de biblioteca**, pero no hay comprobación de vigencia asociada a resultados de CAM ni exportación. El motor de contacto y el muestreo TEST sí existen; **no** hay trayectoria CNC verificada. El contrato v1 utiliza el dominio `LaguCAM/tool-geometry/SHA-256\0`, versión `u32 LE=1`, discriminantes fijos `u8` (plana=1, esférica=2, V=3, cónica plana=4, cónica esférica=5), las dimensiones específicas de cada variante en el orden de su declaración, y finalmente longitud de corte, diámetro de mango y longitud total. Los `f64` se escriben mediante IEEE-754 little-endian, normalizando el cero con signo y rechazando geometría inválida. Un cambio del contrato requiere nueva versión y vectores de prueba.

El contacto esférico con STL se calcula a partir de los valores `f32` originales convertidos a `f64`, nunca desde la normalización gráfica. El BVH usa límites XY y posiciones de registro para no guardar toda la malla dos veces; su presupuesto no incluye toda la RSS del proceso. La fachada del servicio de contacto tiene protección de inmutabilidad de archivo implementada en Windows; otras plataformas siguen pendientes.

**Precaución:** cada punto muestreado puede ser un contacto válido sin que el **movimiento entre puntos** lo sea. El muestreo TEST no verifica el volumen barrido ni colisiones de mango y portaherramientas.

## 6. Capas de arquitectura: componentes experimentales y futuros

```text
      Interfaz egui + Estado del proyecto
                     │
      Operaciones → Resolver UUID en biblioteca (pendiente)
                     │
      Estrategias CAM 3 ejes (TEST inicial)
                     │
       Servicios geométricos independientes (STL experimental)
               ┌─────┴─────┐
           STL (inicio)  STEP/B-Rep (futuro)
                     │
      Trayectoria neutral con dependencias
                     │
   Verificación de setup, colisiones y máquina
                     │
      Postprocesador (FluidNC primero)
                     │
               G-code comprobado
```

Una estrategia dependerá de **capacidades geométricas**, no solo de la extensión del modelo. No forzar STEP a triangulación universal. Los índices espaciales y heightfields auxiliares podrán utilizarse con tolerancia/error explícitos y pruebas independientes, sin comprometer calidad del mecanizado.

El fresado inicial tendrá tres ejes, herramienta paralela a Z del setup, avance de corte habitual hacia −Z. Reservar extensibilidad sin implantar cinco ejes ahora. **Máquina** y **postprocesador** serán bibliotecas independientes: FluidNC primero; la máquina de pruebas cambia fresas manualmente, otras podrán tener ATC. No asumir soporte universal `M6` ni rotación por `G54`.

## 7. Riesgos e invariantes

1. UUID con formato válido no equivale a herramienta existente; resolver o bloquear antes del CAM.
2. El SHA-256 ya existe, pero **sin resolver UUID ni comprobar todas las dependencias** no puede declararse válida una trayectoria.
3. El mango actual no determina salida de pinza; faltan cuello, portaherramientas, sujeciones y comprobación de colisión.
4. `f32` normalizado y depth buffer son visuales, no precisión CAM.
5. `.lagucam` v1 se rechaza: **durante desarrollo no se exige compatibilidad**, pero se documentan roturas y no se borran datos personales silenciosamente.
6. La compilación no demuestra seguridad CNC; toda salida futura requerirá revisión y pruebas controladas.

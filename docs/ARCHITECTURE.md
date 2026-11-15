# Arquitectura de LaguCAM

## Arquitectura CAM integrada localmente (15-11-2026)

- `src/lib.rs` y `src/cam_geometry/*`: motor de consultas geométricas STL independiente de la GUI, con índice BVH, lectura acotada y contacto de punta esférica. Las coordenadas de cálculo proceden del STL original en `f64`, no del render normalizado.
- `src/project/operations.rs` y `src/project/persistence.rs`: operaciones con estrategia opcional, parámetros propios y guardado en `.lagucam`.
- `src/test_parallel.rs`: generación de muestras de «Acabado paralelo — TEST» en segundo plano y previsualización; consulta al motor, sin postprocesador, colisiones ni G-code.
- `src/project_ui.rs`, `src/panel_layout.rs` y `src/main.rs`: árbol, propiedades, apertura del editor único con pestañas y representación 3D. Los módulos `sample_*` y `section_overlay.rs` son diagnóstico temporal, no estrategias de producción.

Este apartado corresponde a `7de83c9` en la rama local de integración. Los apartados del documento con referencia explícita a `f1e436b` describen un corte de código anterior.

## Huella geométrica SHA-256 v1 — ajuste 020, sin acoplamiento CAM
`src/tools/hash.rs` recibe `&Tool` y devuelve 32 bytes SHA-256; usa `ToolGeometry::validate` y valida dimensiones comunes, sin `Tool::validate` (que depende de identidad/metadatos). `sha2` 0.10.9 implementa SHA-256 portable.
Contrato v1 independiente de Serde/JSON/plataforma: `b"LaguCAM/tool-geometry/SHA-256\0"`, versión `u32 LE = 1`, tipo `u8` (plana=1, esférica=2, V=3, cónica plana=4, cónica esférica=5), campos de variante en orden declarado y finalmente longitud de corte, diámetro de mango, longitud total. Cada valor `f64` se codifica con `to_bits().to_le_bytes()`; cero con signo se normaliza a +0.0, NaN/infinito/cotas inválidas se rechazan antes de codificar.
**No forma parte del proyecto `.lagucam`, ni de operaciones ni de trayectorias**. Igualdad de hash geométrico no garantiza validez de G-code: faltaría comparar todas las dependencias CAM. Cambiar el contrato requiere incrementar su versión y renovar los vectores de prueba.

**Estado del código:** [`f1e436b`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee) · 14-11-2026. Se distingue **[ACTUAL]** de **[ACORDADO, NO IMPLEMENTADO]** y **[FUTURO]**.

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
| `project/mod.rs`, `model.rs`, `stock.rs`, `origin.rs`, `display.rs`, `selection.rs`, `operations.rs` | Estado del proyecto, modelo, bruto, cero CNC y operaciones marcadoras |
| `project/persistence.rs` | Serialización `.lagucam` v2, validación y comprobación de STL externo |
| `project_ui.rs` | Árbol lateral y propiedades |
| `tools/model.rs` | Definición y validación de cinco tipos de fresas, UUID y perfiles geométricos `f64` |
| `tools/storage.rs` | Biblioteca global versionada y persistencia independiente |
| `tools_ui.rs`, `tools_profile.rs` | Editor/filtros y perfil gráfico de las fresas |
| `panel_layout.rs` | Paneles flotantes/acoplables y distribución persistente |
| `portable.rs`, `options.rs` | Configuración local y tema claro/oscuro |
| `main.rs` | Coordinación de UI, flujo de archivos, importación en segundo plano y render |

`main.rs` y `panel_layout.rs` son extensos, pero no conviene refactorizarlos solo por longitud. **Las estrategias y las consultas CAM futuras no deberán residir en `main.rs`**.

## 3. Geometría y render [ACTUAL]

```text
STL binario externo → validar y obtener límites f64 → triángulos por bloques
                    → canal acotado → VBO de OpenGL → render STL
                    → render del bruto con profundidad → overlays/UI
```

La GPU conserva las caras renderizadas, pero su geometría normalizada en `f32` **no es la representación geométrica de autoridad para CAM**. La lectura del depth buffer sirve solo para navegación visual. El STL fuente se conserva sin modificaciones; el importador no verifica que sea cerrado, manifold o apto para mecanizado.

El bruto es un prisma alineado con los ejes de modelo, con límites y extras por cara `f64`. El sistema de trabajo usa anclas en modelo/bruto, offsets y `R = Rz · Ry · Rx`, con `p_modelo = origen + R · p_CNC`. Una rotación matemática del marco no significa que `G54` rote ejes ni que una CNC de tres ejes pueda orientar el husillo.

`.lagucam` v2 guarda el proyecto y referencia externa del STL, no la malla ni la biblioteca global. Las operaciones contienen todavía solo marcadores y `tool_id: Option<ToolId>`; **no existe selector operativo ni resolución de herramientas**.

## 4. Modelo de fresas [ACTUAL]

`tools::model::ToolId` es un alias de `String` con formato UUID, validado por `validate_tool_id`. Biblioteca, `Operation` y `SavedOperation` comparten este alias. La biblioteca asigna UUIDv4 y aumenta revisión al editar.

`ToolGeometry` describe fresa plana, esférica, V, cónica de punta plana y cónica de punta esférica. Sus parámetros están en `f64`. El método `profile()` produce segmentos para **representación 2D**, no una superficie exacta que deba reutilizarse para contacto CNC.

La biblioteca es la **única fuente editable**. Ninguna operación conserva una copia editable ni parámetros ocultos de fresa; no hay número T intrínseco.

## 5. Huella geométrica [CÓDIGO PREPARADO EN 020] y dependencias CAM [FUTURO]

Crear una función independiente para obtener SHA-256 sobre representación **binaria canónica y versionada** de los parámetros geométricos efectivos: variante, diámetros, radios, ángulos, longitud de corte, diámetro de mango y longitud total, ampliables con otras cotas físicas que influyan en contacto o colisión. Excluir UUID, revisión, nombre, notas, RPM y avances recomendados. Normalizar valores equivalentes y rechazar no finitos.

En un futuro cálculo se tomará una **copia temporal inmutable** de la definición de biblioteca para impedir cambios durante el cálculo. La trayectoria registrará UUID, hash geométrico y dependencias (modelo, bruto, setup, estrategia, versión de algoritmo). Se bloqueará exportación de resultados incompatibles. Ø6 → Ø4 → Ø6 puede restaurar un hash de herramienta idéntico, **pero eso no demuestra por sí solo vigencia de la trayectoria ni del G-code**.

El cálculo CAM, invalidación y exportación siguen sin existir. La función de hash y sus pruebas están preparadas en el ajuste 020; `.lagucam` permanece sin cambios.

## 6. Capas futuras [FUTURO]

```text
      Interfaz egui + Estado del proyecto
                     │
      Operaciones → Resolver UUID en biblioteca
                     │
            Estrategias CAM 3 ejes
                     │
       Servicios geométricos independientes
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
2. Sin comprobación de dependencias CAM no puede declararse una trayectoria válida, aunque exista la función SHA-256.
3. El mango actual no determina salida de pinza; faltan cuello, portaherramientas, sujeciones y comprobación de colisión.
4. `f32` normalizado y depth buffer son visuales, no precisión CAM.
5. `.lagucam` v1 se rechaza: **durante desarrollo no se exige compatibilidad**, pero se documentan roturas y no se borran datos personales silenciosamente.
6. La compilación no demuestra seguridad CNC; toda salida futura requerirá revisión y pruebas controladas.

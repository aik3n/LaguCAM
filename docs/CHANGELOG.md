# Historial de LaguCAM

## Ajuste 008 v6 — Perfil siempre visible y longitud total (sin nuevo commit)
- Perfil técnico permanente en Herramientas, con esquema **sin escala** cuando no hay medidas.
- Campo de longitud total desde la punta al final del mango representado. Prueba de persistencia migrada al nuevo nombre.
- Compatibilidad de lectura con `free_length_mm`; revisar los datos históricos si eran salidas de pinza.
- Sin cambio de importador STL, OpenGL, operaciones ni `.lagucam`.
- Compilación y prueba gráfica pendientes del equipo Windows.

## 2026-11-14 — Biblioteca global de herramientas, primera implementación (sin commit)

- **Base:** `1e5d5868547fb49e7d151e3d4402d9edfdd1c87f`.
- Cinco tipos de fresa con perfiles geométricos `f64` y validación; ID UUID y revisión; biblioteca global JSON versionada con sustitución segura; UI de listado, filtros, editor y dibujo técnico.
- Sin modificación de STL, renderizador o proyectos `.lagucam` existentes. No se asigna número T ni herramientas a operaciones.
- **Pruebas:** el instalador ejecuta `fmt`, `clippy`, `test`, `build --release` en el equipo Windows; resultados pendientes de su ejecución. No hay validación gráfica ni benchmark de STL grande en esta entrega.
- **Documentos actualizados:** LIBRARIES, ARCHITECTURE, STATUS, REQUIREMENTS, ROADMAP, DECISIONS, CHANGELOG.

Este documento resume commits y cambios visibles de la línea base. **No es un listado exhaustivo de todos los cambios**; consultar GitHub para la historia completa. No equivale a informe de pruebas ejecutadas en la revisión documental.

## 2026-11-14 — Primera base documental (sin commit Git)

- Preparados documentos de visión, requisitos, arquitectura, decisiones, librerías, roadmap, estado, pruebas y protocolo de trabajo.
- Creado `AGENTS.md` para describir el comportamiento de los chats de dirección y desarrollo.
- Fotografiado el estado de código en `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`.
- Registrada evidencia de usuario: STL de casi 1 GB visualizado y navegado con fluidez en su equipo.
- Registradas discrepancias entre documentación antigua y código actual (README, CI).
- **No se han modificado archivos en el repositorio remoto. No se han ejecutado pruebas del programa en esta tarea de documentación.**

## Commits de referencia

| SHA | Fecha UTC de commit | Mensaje y cambio relevante |
| --- | --- | --- |
| [`0e56d3f`](https://github.com/aik3n/LaguCAM/commit/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9) | 2026-11-14 | «puntos de origen»: marcadores de selección del cero CNC, referencias de origen y color del STL |
| [`c482baa`](https://github.com/aik3n/LaguCAM/commit/c482baaa8b3737ef9cbf5484171331c4c10447e4) | 2026-11-14 | «definir bruto y alguna mejora»: sólido translúcido/aristas, color y opacidad, render independiente |
| [`016c618`](https://github.com/aik3n/LaguCAM/commit/016c618957f5b4a25edfb906bd20441ae7dfae6f) | 2026-11-14 | «definir bruto»: árbol de proyecto, extras y sistema de referencia |
| [`44413d5`](https://github.com/aik3n/LaguCAM/commit/44413d57f09a2d50bb4c3a30d1a86be52d360e33) | 2026-11-13 | «stl sin limites»: representación completa por bloques y carga progresiva GPU |
| [`9300580`](https://github.com/aik3n/LaguCAM/commit/9300580c497b808cf5d8371c56c27ffaeef84357) | 2026-11-13 | «cargar stl»: primeros cambios de importación |

Estas descripciones se basan en commits y documentación asociada; **no se certifica compilación ni aceptación de cada SHA** solo por aparecer aquí.

## Plantilla de entrada futura

```text
## AAAA-MM-DD — <nombre de hito> [<SHA>]
Objetivo:
Cambios realizados:
Decisiones afectadas: D-...
Requisitos afectados: ...
Pruebas ejecutadas: <comandos y resultado>
Validación del usuario: <sí/no/pendiente + alcance>
Regresiones/riesgos:
Documentos actualizados:
```

La documentación se actualiza con el cambio; no añadir pruebas inexistentes ni cerrar hitos sin aceptación.

## 2026-11-14 — Ajuste 009: acceso global y cielo degradado (sin commit)

- Base Git: `1e5d5868547fb49e7d151e3d4402d9edfdd1c87f`, biblioteca 008 v6 ya instalada localmente.
- La biblioteca pasa al menú principal `Herramientas`; se elimina el nodo del árbol del proyecto, la variante de selección obsoleta y el retorno booleano del panel de proyecto.
- Decisión D-024: operaciones futuras con referencia al UUID global, nunca snapshots ni parámetros ocultos. D-010 sustituida. No se implementan aún operaciones ni cambios de formato `.lagucam`.
- Fondo de visor en degradado azul cielo mediante malla egui; el renderizador borra únicamente profundidad. STL grande, stock, origen y rendimiento pendientes de validación gráfica.
- Controles Rust: el instalador ejecuta `fmt --check`, `clippy -D warnings`, `test --locked`, `build --release`; resultados por confirmar en el equipo Windows.

## 2026-11-14 — Pivote visual independiente del formato (010)

- **Base:** `46f66fce07236e296a0c92dc27645b3b2e3e6cf8`. Sin commit nuevo.
- Doble clic para elegir un pivote de rotación sobre una superficie renderizada, sin leer ni alterar STL, malla, bruto, origen CNC o datos `.lagucam`.
- Matriz de cámara compartida y servicio de consulta de profundidad 1×1 compatible con MSAA; restituye estado GL al terminar y no utiliza memoria proporcional al número de caras.
- Pivote recuperable desde `Vista > Centrar vista` y marca discreta cuando hay pivote personalizado.
- **Pruebas:** el instalador ejecutará fmt, clippy, test y release en el equipo del usuario; prueba funcional OpenGL y regresión STL grande pendientes.

## 2026-11-14 — Pivote por clic simple sin salto (011)

- Base de Git: `46f66fce07236e296a0c92dc27645b3b2e3e6cf8` con la 010 v2 instalada localmente. Sin commit automatico.
- Sustituido doble clic por clic simple; se elige el pivote sin alterar la proyeccion actual. El siguiente arrastre orbita alrededor de ese punto y la marca se muestra donde realmente fue seleccionado.
- Se mantiene el servicio de consulta de escena independiente de los importadores y no se modifican STL grande, bruto, origen CNC, biblioteca ni formato `.lagucam`.
- Pruebas matematicas adicionales; compilacion, interfaz GPU y regresion de STL grande pendientes en la instalacion Windows.

## 2026-11-14 — Ajuste 012: mouse down y giro inmediato

- Base verificada: `a1532c456173c10341715067072e303b45de9150`. Sin nuevo commit automatico.
- El boton izquierdo fija el pivote en la pulsacion inicial; manteniendolo pulsado se gira en el mismo gesto. El movimiento anterior al resultado de picking se conserva y se aplica despues del nuevo pivote.
- La marca se sigue dibujando a partir del pivote persistente, sin condicion de boton pulsado. Clic en fondo: conservar pivote.
- Sin cambios en formato `.lagucam`, STL, GPU streaming, biblioteca de fresas ni origen CNC.
- Pruebas de estado de gesto incorporadas; Cargo y validacion grafica pendientes de Windows.

## 2026-11-14 — Ajuste 013: marcador temporal de pivote

- Base: 012 v2 instalada sin commit sobre HEAD `a1532c456173c10341715067072e303b45de9150`.
- Pulsar sobre el modelo sigue seleccionando el punto de giro; el mismo gesto sigue arrastrando en torno a el. Solo cambia la visibilidad del indicador: al soltar se oculta pero el pivote permanece activo.
- `NavigationBindings` reune los botones de orbita y desplazamiento, con valores actuales por defecto y sin introducir todavia ajustes persistentes.
- Se agregan pruebas de ciclo de vida del indicador y mapeo por defecto. No hay cambios en CAM, STL, origen CNC, almacenamiento `.lagucam` ni herramientas.

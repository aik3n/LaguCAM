# Historial de LaguCAM

## 2026-11-17 — Sincronización documental y procedimiento local por parches

- **Base de código público inspeccionada:** `5cd41c7` (no es un SHA nuevo del parche documental). Inspección estática, **sin ejecución propia de Cargo, GUI o CNC**.
- Evolución acumulada posterior a `13a439a`: ContactSolver G19–G24 para esférica, plana y toroidal; biblioteca UUID conectada al CAM y SHA-256 de herramienta para previews; sobreespesor y patrón de pasadas.
- G25 incorpora segmentos de corte neutros, enlaces propuestos, marcadores de inicio/fin y validación **estructural**. F1.1–F1.2 añaden contrato de operación F/S y marco de trabajo Rz/traslación; **no** post ni verificación integral de colisiones.
- UI: cubo de orientación, zoom/pivote, proyectos recientes; proyectos `.lagucam` sin backups de recuperación automática por decisión del usuario.
- **D-037–D-039:** los chats no usan Git, modifican la carpeta local exclusivamente mediante **parches `.ps1` colocados en el Escritorio junto a `LaguCAM`**, entregando comandos de ejecución, compilación y tests. El usuario aplica, valida y hace commits. Mantener D-036: documentar después de la validación.
- Evidencia diferenciada en `STATUS.md`: las pruebas anteriores se atribuyen a su SHA/ejecutor; no se certifica precisión ni rendimiento CAM sobre STL real por inspección del repositorio.


## 2026-11-15 — Motor CAM experimental y consolidación (`7de83c9`, `13a439a`)

- `7de83c9`: se añade `cam_geometry/*` (contacto esférico, BVH STL, lectura por bloques/caché, cancelación y límites), operaciones y muestreo de acabado paralelo TEST con previsualización. **No** G-code, trayectoria CNC segura ni postprocesador.
- `13a439a`: consolida `main`, limpia avisos menores y publica notas documentales. Los documentos que describían la rama `integracion/cam-20261115` como «todavía no publicada» reflejan un momento histórico anterior.
- Evidencia **comunicada**, no reproducida por quien documenta: 130 pruebas correctas, `cargo fmt --check`, `cargo check` y `cargo build --release` en Windows sobre la base `7de83c9`, más pruebas de operaciones. No hay medición certificada del CAM sobre el STL real ~1 GB.
- Falta integrar herramientas UUID/hash, tolerancias de recorrido, colisiones, trayectoria neutral, FluidNC y seguridad de máquina.

## 2026-11-14 — Hash geométrico y tema (`8e35c0e`)

- Función `src/tools/hash.rs` SHA-256 canónica v1, con pruebas de cinco geometrías y recuperación Ø6→Ø4→Ø6. Aún no conectada a cálculo/exportación CAM.
- Corrección de preferencias de tema claro/oscuro en egui. La prueba gráfica y de regresión debe atribuirse a su ejecutor.

## 2026-11-15 — Proceso documental aprobado (D-036)

- Se mantiene debate → aceptación → implementación → validación → documentación. El código se entrega sin editar Markdown; este chat documenta tras validar un hito.
- `docs/STATUS.md` es la única fuente del estado documentado, con SHA de código cubierto y evidencias. Diferencias temporales con HEAD son normales y explícitas.
- Sin cambios funcionales en este paquete documental. Basado en `13a439a`; **no consta commit remoto de esta entrega**.


## Documentación consolidada — 14-11-2026 (registro histórico previo a la publicación)

- **Base de código inspeccionada:** [`f1e436b`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee), «ajuste htas».
- Documentados lector STL grande, persistencia `.lagucam` v2, cinco geometrías de fresas, UUID global, configuración portable, docking y temas.
- Incluidas las decisiones D-024–D-032 registradas en el desarrollo, D-033 (sin compatibilidad obligatoria durante desarrollo) y D-034 (huella geométrica SHA-256: aceptada pero **sin código**).
- Corrección de afirmaciones históricas del README sobre límite del STL y CI. Documentación y pruebas **no se han publicado/ejecutado** en esta preparación local.

## Ajuste 019 — UUID compartido, commit [`f1e436b`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee)

- `tools/model.rs`: `ToolId = String` y validación de UUID centralizada; `Operation` y `SavedOperation` usan ese identificador.
- `.lagucam` pasa a **versión 2**: UUID textual o `null` para `tool_id`; documentos v1 se rechazan expresamente, comportamiento permitido durante desarrollo.
- Pruebas de serialización, UUID incorrecto y versión antigua incluidas en el código; **no se ejecutaron aquí**. No se modificó la geometría del motor STL ni se incorporó CAM.


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
| [`f1e436b`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee) | 2026-11-14 | «ajuste htas»: ToolId compartido, validación UUID, `.lagucam` v2; sin migración v1 |
| [`599ca50`](https://github.com/aik3n/LaguCAM/commit/599ca50eec0c6a716d9bd5ffdf7511c35d94df7d) | 2026-11-14 | «mejorar paneles mobiles, cerrar ui de momento»: asa visual y docking |
| [`020016a`](https://github.com/aik3n/LaguCAM/commit/020016a0904a9e484f81accc9859d8a884061c30) | 2026-11-14 | «tema claro/oscuro»: apariencia configurable |
| [`836cd1d`](https://github.com/aik3n/LaguCAM/commit/836cd1dd17b4588d23b72c38577c92e4b2257661) | 2026-11-14 | «ui paneles mobiles»: evolución de paneles |
| [`cff5fc9`](https://github.com/aik3n/LaguCAM/commit/cff5fc999c7faa671371e25e82f236ec94d7e570) | 2026-11-14 | «posicion paneles»: acoplamiento de paneles |
| [`185b99a`](https://github.com/aik3n/LaguCAM/commit/185b99a5c50b9873efb09a3bb756212f2f2cfe98) | 2026-11-14 | «mejora pibote camara»: gestos de navegación |
| [`a1532c4`](https://github.com/aik3n/LaguCAM/commit/a1532c456173c10341715067072e303b45de9150) | 2026-11-14 | «mejora rotacion camara»: interacción orbital |
| [`46f66fc`](https://github.com/aik3n/LaguCAM/commit/46f66fce07236e296a0c92dc27645b3b2e3e6cf8) | 2026-11-14 | «biblioteca htas»: primera biblioteca global de fresas |
| [`1e5d586`](https://github.com/aik3n/LaguCAM/commit/1e5d5868547fb49e7d151e3d4402d9edfdd1c87f) | 2026-11-14 | «add doc»: primera documentación en repositorio |
| [`6eb12e1`](https://github.com/aik3n/LaguCAM/commit/6eb12e156d3538b73c4d9358c0b4c108c357fc5b) | 2026-11-14 | «abrir y guardar»: persistencia inicial `.lagucam` |
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

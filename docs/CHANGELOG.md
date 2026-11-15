# Historial de LaguCAM

## 15-11-2026 — Integración local del motor geométrico y operaciones TEST

- Rama `integracion/cam-20261115`, commit base `7de83c9` (sin publicación remota): 15 archivos integrados; motor de contacto esférico, extracción de secciones, vistas de diagnóstico, operaciones parametrizables y persistencia.
- La operación se crea vacía en el árbol, se edita con doble clic en una ventana con pestañas y puede eliminarse desde su propio editor. El resultado es previsualización geométrica, no G-code.
- Windows: 130 pruebas (20+104+6) superadas, `cargo fmt --check`, `cargo check` y compilación Release. Validación visual comunicada: abrir STL y crear/modificar/eliminar operaciones en el LaguCAM original.
- Consolidación local: limpieza de cuatro avisos Rust y actualización de documentación, conservando el commit funcional previo como referencia.
- No se ha fusionado con `main` ni publicado en GitHub. Sin cambios a STL personales o proyectos existentes.

## Ajuste 021 — Preferencia de tema aplicada y restaurada correctamente
- Corregido `src/options.rs`: la preferencia Claro/Oscuro selecciona `egui::Theme` con `Context::set_theme`, además de aplicar su paleta con `set_visuals_of`.
- En egui 0.37 `set_visuals` por sí solo no selecciona el tema; el tema del sistema podía quedar activo aunque el selector mostrara el contrario.
- Se mantiene `config/settings.json` v1 y el guardado portable. Sin cambios en operaciones, hash geométrico 020, `.lagucam`, docking, visor o CAM.
- Añadidas pruebas para selección efectiva de tema y restauración de la preferencia. Cargo y ensayo gráfico: se ejecutan en el Windows del usuario, no previamente aquí.
## Ajuste 020 — Huella geométrica SHA-256 v1 (base db5ee13, entrega sin commit)
- Nuevo `tools/hash.rs`: función pura `geometry_sha256(&Tool) -> Result<[u8; 32], String>`, sin serialización JSON ni dependencia de UI/CAM. Usa SHA-256 de RustCrypto (`sha2` 0.10.9, MIT/Apache-2.0).
- Contrato binario v1: dominio ASCII terminador NUL, versión u32 little-endian, discriminante u8 por tipo y f64 IEEE-754 little-endian de dimensiones en orden documentado; ceros normalizados. Cotas no finitas/no válidas se rechazan.
- Incluye tipo, diámetros, radios, ángulos y dimensiones comunes; excluye UUID, revisión, nombre, notas, referencia comercial y recomendaciones de corte.
- Se añaden pruebas de cinco vectores SHA-256 conocidos, sensibilidad de cada cota, cambios no geométricos, Ø6→Ø4→Ø6 y validación. Cargo y prueba gráfica: pendientes de ejecutar en Windows con instalador.
- Sin cambios en operaciones, formato `.lagucam`, UI, OpenGL, motor CAM o identidades de fresas. `Cargo.lock` se actualizará en la instalación y se incluirá en la copia de seguridad.

## Documentación consolidada — 14-11-2026, sin commit remoto

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

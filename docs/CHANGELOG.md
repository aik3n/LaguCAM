# Historial de LaguCAM

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

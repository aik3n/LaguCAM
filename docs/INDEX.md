# Documentación de LaguCAM

**Corte documental:** 2026-11-14 · **Repositorio:** https://github.com/aik3n/LaguCAM  
**Último commit revisado:** [`0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`](https://github.com/aik3n/LaguCAM/commit/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9)

Este conjunto resume las decisiones de las conversaciones de dirección técnica y las contrasta con los archivos accesibles del repositorio. Se entrega **por separado**; no se ha publicado ni modificado el repositorio GitHub.

## Índice y orden recomendado de lectura

| Documento | Finalidad |
| --- | --- |
| [`../AGENTS.md`](../AGENTS.md) | Comportamiento esperado de los chats y agentes |
| [`VISION.md`](VISION.md) | Propósito, alcance y principios |
| [`REQUIREMENTS.md`](REQUIREMENTS.md) | Requisitos funcionales y no funcionales, con estado |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Arquitectura implementada y arquitectura futura prevista |
| [`DECISIONS.md`](DECISIONS.md) | Registro de decisiones aceptadas, propuestas y abiertas |
| [`LIBRARIES.md`](LIBRARIES.md) | Diseño de herramientas, máquinas y postprocesadores |
| [`ROADMAP.md`](ROADMAP.md) | Etapas, dependencias y criterios de salida |
| [`STATUS.md`](STATUS.md) | Estado verificable del software y riesgos activos |
| [`TESTING.md`](TESTING.md) | Política de pruebas y seguridad CNC |
| [`WORKFLOW.md`](WORKFLOW.md) | Procedimiento de colaboración y actualización por commit |
| [`CHANGELOG.md`](CHANGELOG.md) | Resumen histórico de cambios y trazabilidad |

## Convenciones de evidencia

- **USUARIO-VALIDADO**: comprobación comunicada por el usuario en su máquina; no equivale a una medición documentada de FPS, RAM o precisión geométrica.
- **CÓDIGO-VERIFICADO**: función observada en archivos del commit indicado; no afirma que la experiencia gráfica se haya ensayado.
- **INFORME-DECLARADO**: dato de README, auditoría o documento de una entrega; exige comprobarse si resulta crítico.
- **ACORDADO**: decisión tomada durante el debate; puede no existir implementación.
- **PROPUESTO**: recomendación aún no aceptada como requisito.
- **PENDIENTE**: no se ha encontrado implementación o no se ha validado.

Si hay conflicto entre documentación antigua y código actual, **prevalece el código para describir lo implementado**, pero no autoriza a considerar correcto el comportamiento sin las pruebas pertinentes. Registrar la discrepancia y corregir los documentos.

## Fuentes de la fotografía inicial

- Código Rust en `src/` del commit de referencia, especialmente `stl.rs`, `preview.rs`, `render.rs`, `stock_render.rs`, `project/*`, `project_ui.rs`, `overlay.rs` y `main.rs`.
- `AUDITORIA.md`, `PROJECT_TREE.md`, `LAGUCAM_004.md`, `LAGUCAM_005.md` y `LAGUCAM_006.md`.
- Conversaciones y pruebas comunicadas por el responsable del proyecto; capturas de configuración de DeskProto como **referencia de interacción**, no como diseño a copiar ni prueba de capacidades de LaguCAM.

## Actualización obligatoria

Cada cambio funcional significativo debe actualizar `STATUS.md` y `CHANGELOG.md`, revisar `REQUIREMENTS.md` y `ROADMAP.md`, y modificar `ARCHITECTURE.md`, `LIBRARIES.md` o `DECISIONS.md` cuando corresponda. Ver el detalle en `WORKFLOW.md`.

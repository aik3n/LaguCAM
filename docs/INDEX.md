# Documentación de LaguCAM

El documento principal del **último corte documentado** es [`STATUS.md`](STATUS.md). Declara el SHA público inspeccionado y las pruebas acreditadas; el código local del Escritorio puede ser anterior o posterior. **Los chats entregan parches `.ps1` y no realizan operaciones Git.** Ver [`WORKFLOW.md`](WORKFLOW.md).

## Orden de lectura

1. [`STATUS.md`](STATUS.md) — Estado y evidencia hasta un SHA preciso.
2. [`DECISIONS.md`](DECISIONS.md) — Acuerdos aprobados y decisiones abiertas.
3. [`ARCHITECTURE.md`](ARCHITECTURE.md) — Componentes, límites e invariantes técnicos.
4. [`ROADMAP.md`](ROADMAP.md) — Hitos pendientes y criterios de salida.
5. [`WORKFLOW.md`](WORKFLOW.md) — Ciclo de cinco fases, parche PowerShell desde Escritorio, tests Windows y Git exclusivo del usuario.

## Documentación de referencia

| Documento | Propósito |
| --- | --- |
| [`REQUIREMENTS.md`](REQUIREMENTS.md) | Requisitos y criterios técnicos, no un segundo estado del código |
| [`LIBRARIES.md`](LIBRARIES.md) | Bibliotecas de herramientas, máquinas y postprocesadores |
| [`TESTING.md`](TESTING.md) | Pruebas, benchmarks y requisitos de seguridad CNC |
| [`CHANGELOG.md`](CHANGELOG.md) | Historial de entregas y commits, no estado vivo |
| [`VISION.md`](VISION.md) | Objetivo de largo plazo y alcance |
| [`CAM_INTEGRACION_2026-11-15.md`](CAM_INTEGRACION_2026-11-15.md) | Informe histórico de integración experimental del motor; no sustituye STATUS |
| [`../AGENTS.md`](../AGENTS.md) | Reglas para chats de dirección, programación y motor |
| [`../README.md`](../README.md) | Introducción, compilación y enlaces |

## Interpretación del estado

- **IMPLEMENTADO:** código presente y revisado, no necesariamente validado en ejecución.
- **PRUEBA REPORTADA:** resultado atribuido a su ejecutor y SHA; no verificado independientemente por quien documenta.
- **VALIDADO POR EL USUARIO:** prueba concreta en el hardware y escenario descritos; no representa garantía universal.
- **PENDIENTE / FUTURO:** funcionalidad no terminada, no comprobada o todavía propuesta.

La documentación **no debe simular sincronización** con commits posteriores ni con parches locales pendientes. Al reanudar una tarea, inspeccionar primero **archivos locales reales** o declarar expresamente que solo se dispone del SHA público. Respetar las decisiones hasta su sustitución formal.

Los archivos históricos `AUDITORIA.md` y `DEPENDENCIAS.md` se conservan como referencia de etapas anteriores. Los documentos `LAGUCAM_004.md` a `LAGUCAM_007.md`, `PROJECT_TREE.md` e `INSTALACION_DOCUMENTACION.md` **se eliminaron** y no forman parte del índice vigente.

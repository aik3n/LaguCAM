# Traspaso operativo del Chat Master — LaguCAM

<!-- LAGUCAM-MASTER-DOC-CUT: 85dfda28658d5f8e779caae97838742d1444be73 -->

**Corte preparado:** 2026-11-18. **HEAD publico inspeccionado:** `85dfda28658d5f8e779caae97838742d1444be73` (`main`, «operaciones multiples»). Base local no inspeccionada; aplicar parche `.ps1` de documentacion es responsabilidad del usuario. **Sin ejecucion propia de cargo, GUI, benchmark ni CNC sobre ese SHA.** El commit documental posterior no existe mientras el usuario no lo realice.

## Autoridad y lectura

- Usuario: decide prioridades, acepta alcance, ejecuta parches y tests en Windows, realiza todo Git y autoriza cualquier prueba en maquina.
- Master: audita, documenta y coordina; no realiza implementacion de codigo salvo delegacion expresa.
- Integrador CAM: responde del recorrido completo. Especialistas Motor, Acabado, Post/Maquina y UI/UX intervienen por contratos. Desbaste permanece pospuesto.
- Orden: `../AGENTS.md` → `WORKFLOW.md` → `CHAT_ROLES.md` → `DECISIONS.md` → `STATUS.md` → `ROADMAP.md` → `ARCHITECTURE.md` → `TESTING.md`.
- `STATUS.md` es la unica referencia de estado y evidencias; `ROADMAP.md` tareas; `DECISIONS.md` acuerdos y propuestas. No deducir resultados de test desde un commit.

## Hitos mas recientes inspeccionados

| SHA | Novedad | Garantia real |
| --- | --- | --- |
| `59e8f1a` | Primer post/NC experimental y exportacion | No NC apto para mecanizado |
| `de8c0b6` / `d32101e` / `1c64ceb` | Interpretacion NC, simulacion 2,5D, visor OpenGL y comprobacion preliminar | G53 y colisiones reales aun no cubiertos |
| `d108dfad` | Identidad de sesion y correspondencia UUID/hash de herramienta | Requiere regresion GUI y pruebas integradas |
| `0f4c811` | UX de operacion individual y resumen/avisos NC | Tiempo G1 no es tiempo fisico de ciclo |
| **`85dfda2`** | Calculo conjunto de operaciones activas, cache, cancelacion, indices | **Sin NC conjunto ni simulacion conjunta** |

## Decisiones nuevas del responsable (2026-11-18)

- **D-045 ACEPTADA:** cada estrategia comienza en su XY de operacion a Z de seguridad sobre el bruto definida por la propia estrategia; prohibido mover XY para buscar aparcamiento/homing de maquina. Enlaces internos entre pasadas siguen permitidos a despeje adecuado.
- **D-046 ACEPTADA como reparto de responsabilidad:** posicion inicial, homing, montaje y origen fisicos del operador; retirada inicial **opcional** solo Z mediante G53, configurada en post/perfil de maquina, sin cota universal ni seguridad presumida. En este SHA `nc_interpreter.rs` rechaza G53 y un Start personalizado puede impedir analisis automatico.
- **D-047 PROPUESTA:** planificador global para transiciones entre operaciones; no atribuirlo a estrategias. No confundir calculo en lote de `85dfda2` con programa NC conjunto.

## Riesgos no cerrados

1. No certificacion de seguridad CNC: colisiones de fresa/portaherramientas/sujeciones y material remanente ausentes; NC de ejemplo pueden contener entradas verticales profundas.
2. G53 no interpretado; perfil de maquina/homing/limites FluidNC real pendiente. Posicion teórica de CAM no es posicion fisica CNC.
3. Validar operaciones activas, reordenacion, error intermedio, cancelacion, guardado/recarga y caducidad en lote; no hay pruebas de ejecucion de `85dfda2` acreditadas.
4. Inspeccionar fallback visual de `preview_matrix()` a identidad y coherencia del modo aislado y render; sin refactorizacion masiva por tamano de fichero.
5. Nombres historicos de hitos en codigo/tests pendientes de limpieza selectiva, conservando cobertura.

## Proximo trabajo concreto

1. Contrastar archivos locales contra `85dfda2`; si hay cambios posteriores, **no** aplicar un parche preparado para otra base sin revisarlo.
2. Solicitar salida real de `fmt`, `check`, `test`, `clippy`, release y pruebas GUI del calculo individual y conjunto; registrar ejecutor, entorno y SHA.
3. Preparar con Integrador/Acabado un test de contrato de punto inicial XY/Z y continuidad; no cambiar estrategia mientras no exista alcance aceptado.
4. Con Post/Maquina proponer contrato de `G53` y estados de maquina, **sin anunciarlo implementado**.
5. Actualizar `HANDOFF.md` nuevamente cuando se validen los siguientes cambios; el usuario decide cuando hacer commit y si cambia de Master.

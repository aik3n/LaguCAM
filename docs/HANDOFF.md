# Traspaso del Chat Master — LaguCAM

**Última preparación:** 2026-11-17 · **Naturaleza:** estado operativo para sucesión; **no es otro `STATUS.md`**.

## Identidad y límites

- **Proyecto:** LaguCAM · https://github.com/aik3n/LaguCAM · Rust/egui/OpenGL · CAM experimental de tres ejes.
- **Último HEAD público inspeccionado en esta revisión:** `a10e0c0d7c95db773417c2b190e7f4ff9005d8d4` (17-11-2026).
- **Base local:** **NO observada** por el Master. El código publicado no acredita el contenido de `Escritorio/LaguCAM`.
- **Estado de aplicación de este parche documental:** **PENDIENTE DE CONFIRMACIÓN** hasta que el usuario lo ejecute y comunique el resultado; **SHA de commit posterior: no existe acreditado en este traspaso**.
- **Pruebas sobre `a10e0c0`:** solo inspección estática de GitHub; **sin ejecución propia de `cargo`, GUI, benchmark ni CNC**, ni CI publicado observado. Nunca atribuirlas al usuario sin informe.

## Hito activo

**P1 — Cerrar contrato e integración CAM.** Responsable de implementación: **Integrador CAM**. Master supervisa y audita. Cambios recientes en código publicado (commits `4a51d65`, `94b6d8c`, `a10e0c0`) incorporan procedencia/invalidez de previews, unidad fuente del STL y transformaciones físicas no destructivas en visor/BVH, además de ajustes de tests. **No se ha certificado P1 como cerrado.**

**Próxima tarea concreta:** pedir al Integrador que contraste la base local y prepare una propuesta *sin programar hasta aceptación* para resolver: (a) fallo silencioso de `preview_matrix()` a identidad visual; (b) invalidación innecesaria de geometría al editar nombre, F/S; (c) pruebas end-to-end de unidades/transformación/guardado/cálculo/trayectoria/origen; (d) contrato explícito de validez antes de cualquier post; (e) benchmark BVH/STL grande con datos medidos. Priorizar por impacto y dividir en parches pequeños.

## Riesgos y límites abiertos

| Tema | Evidencia observada | Estado / medida |
| --- | --- | --- |
| Divergencia visual/CAM | `main.rs` usa `unwrap_or(Mat4::IDENTITY)` si falla `preview_matrix()` | Corregir vía manejo de error o bloquear, no renderizar geometría distinta en silencio |
| Invalidación excesiva | Editor borra `Outcome` por cualquier cambio en `kind`/`name`; `CamProvenance` excluye F/S como dependencia geométrica | Separar validez geométrica de tecnológica cuando exista contrato claro |
| Identidad del STL | FNV-64 muestreado + metadatos para verificación económica | No describir como comprobación criptográfica completa; definir robustez exigida antes de exportar |
| Contactos y recorridos | G25 verifica estructura, no volumen barrido ni colisiones | Mantener exportación operativa bloqueada hasta verificaciones adicionales |
| Máquina FluidNC | Sin perfil operativo ni post | P3 futuro; no asumir `M6`, `G54` ni límites de la máquina |
| Calidad | Hay pruebas unitarias; sin CI ni ejecución de este HEAD acreditada aquí | Requerir salidas Windows, casos integrados y métricas; no extrapolar |
| Acoplamiento | `test_parallel.rs` acumula UI, hilos y resultados | Refactorización **localizada solo cuando facilite un contrato/probabilidad de fallo concreto**, no reescritura general |

## Organización, reglas y referencias de autoridad

- **Usuario** decide, ejecuta parches/validaciones y hace **todo Git**. Los chats no ejecutan Git ni escriben directamente en la carpeta de trabajo: entregan un `.ps1` por avance, colocado en el Escritorio junto a `LaguCAM`.
- **Master** planifica, objeta, audita y documenta; **Integrador** responde del primer flujo completo; **Motor**, **Acabado**, **Post** y **UI/UX** se activan por contrato; **Desbaste** queda en espera.
- **Lectura obligatoria por orden:** [`../AGENTS.md`](../AGENTS.md) → [`WORKFLOW.md`](WORKFLOW.md) → [`CHAT_ROLES.md`](CHAT_ROLES.md) → [`DECISIONS.md`](DECISIONS.md) → [`STATUS.md`](STATUS.md) → [`ROADMAP.md`](ROADMAP.md) → [`ARCHITECTURE.md`](ARCHITECTURE.md) → [`TESTING.md`](TESTING.md).
- **Fuente del último estado técnico:** `STATUS.md` (con SHA y evidencias). **Prioridades:** `ROADMAP.md`. **Acuerdos:** `DECISIONS.md`. Este documento contiene **solo el relevo operativo**.

## Protocolo para el siguiente Master

1. Obtener del usuario la confirmación del último parche aplicado y resultados de pruebas; **no asumir que esta entrega ya se ejecutó**.
2. Leer los documentos normativos y verificar el código real accesible (o declarar que solo se audita GitHub). Comparar el último SHA conocido y parches locales pendientes sin ejecutar comandos Git.
3. Reconciliar hechos con las evidencias: qué está implementado, probado en Windows, validado por el usuario, pendiente o fuera de alcance. Registrar discrepancias sin inventar datos.
4. Mantener **P1** hasta aceptación de sus criterios de salida; no abrir P3 o desbaste por iniciativa propia. Auditoría crítica y proporcional.
5. **Actualizar este `HANDOFF.md` en cada hito documental validado**, conservándolo breve: HEAD público, base local reportada, parche/resultado, hito y responsable, bloqueos, pruebas y próxima decisión. La sustitución de Master requiere designación explícita del usuario, no depende del chat anterior.

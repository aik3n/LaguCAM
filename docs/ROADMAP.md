# Hoja de ruta de LaguCAM — prototipo completo de tres ejes

**Revisión de prioridades:** 2026-11-17. **Código público inspeccionado:** `a10e0c0`. Este documento define prioridades, no concede autorización automática para implementar. Para pruebas y SHA completo consultar [`STATUS.md`](STATUS.md).

## Objetivo de salida del prototipo

Flujo reproducible **STL → proyecto/bruto/origen → fresa de biblioteca → operación de acabado paralelo → contacto geométrico → trayectoria neutral y enlaces → comprobaciones → programa FluidNC → revisión externa**, con visualización y diagnósticos. El primer cierre debe limitarse a **tres ejes y una estrategia**. Que pueda exportarse un archivo experimental no demuestra seguridad de mecanizado.

## Hitos y situación

| Hito | Resultado | Estado observado hasta `a10e0c0` |
| --- | --- | --- |
| 0. Visor y proyecto | Abrir STL grande, navegar, bruto, origen, guardar `.lagucam` | Implementado; uso del visor grande comunicado por el usuario; regresión cuantitativa pendiente |
| 1. Biblioteca de fresas | Identidad UUID, geometrías y conexión CAM | Implementado para esférica, plana y toroidal; verificaciones físicas pendientes |
| 2. Motor geométrico | ContactSolver, sobrematerial, BVH y tests | Prototipo G19–G24 implementado; precisión y rendimiento con STL grande pendientes |
| 3. Primera estrategia | Acabado paralelo con patrones y parámetros | TEST operativo en código; sigue sin verificación completa de recorrido |
| 4. Movimientos neutrales | Segmentos, enlaces, aproximaciones, retiradas | G25 implementado **como propuesta experimental**; validador estructural, no anticolisiones |
| 5. Contrato y marco de trabajo | F/S, modelo → cero CNC, unidades y transformaciones STL | F1.1–F1.2, procedencia P1-A y transformación P1-M2 en código; P1 **abierto** hasta pruebas integradas y correcciones |
| 6. Máquina y post | Perfil mínimo, exportación FluidNC desacoplada | Pendiente; condiciones y capacidades reales de máquina sin acreditar |
| 7. Verificación + UI de salida | Chequeos adicionales, inspección, exportación limitada | Pendiente; condición para terminar prototipo |
| Posteriores | STEP, 4/5 ejes, nuevas estrategias, stock remanente | Fuera del alcance inicial |

## Secuencia recomendada para los próximos chats

### P1 — Cerrar el contrato y la trazabilidad CAM

Aceptar explícitamente unidades STL/mm y conversiones, referencia de trabajo, esquema de operación y F/S, identidad/huella de herramienta, dependencias de bruto/modelo/setup y caducidad de resultados. Verificar que las operaciones y movimientos usan las mismas coordenadas y que los parámetros persistentes se leen correctamente. **Sin post ni G-code aún.**

**Salida:** pruebas unitarias y de integración de transformaciones, origen, cambios de herramienta/parámetros y rechazos explícitos; al menos un caso real de proyecto y previsualización.

### P2 — Cerrar las garantías verificables del recorrido

A partir de G25.4 estructural, diseñar comprobaciones independientes de segmentos entre muestras, enlaces y alturas. Definir tolerancias geométricas, cuerpo de herramienta, obstáculos/utillaje y qué casos no pueden verificarse. **Ante incertidumbre, bloquear la exportación operativa en lugar de emitir una garantía ficticia.**

**Salida:** patrones de referencia, escenarios de colisión y errores deterministas; rendimiento/memoria sobre STL creciente y el grande cuando esté disponible.

### P3 — Perfil mínimo de máquina y primer post FluidNC

Aprobar las capacidades reales del controlador y del equipo de pruebas: ejes, recorridos, homing, G54, límites, RPM/F, parada y **cambio manual**. Construir postprocesador independiente que solo acepte datos con precondiciones comprobadas.

**Salida:** G-code experimental inspeccionable, tests de secuencia/modalidad con archivos golden y rechazo de operaciones no válidas. No presuponer que `M6` cambia la herramienta o que `G54` rota coordenadas.

### P4 — Flujo UI y cierre de prototipo

Integrar configuración, generación, estado de incidencias, visualización y exportación; pruebas end-to-end desde proyecto persistido hasta salida del post. Validar en Windows con STL representativo y simulación offline. El salto a mecanizado real requerirá proceso controlado adicional.

## Coordinación y activación de chats

**Master + Integrador** trabajan ahora sobre P1. **Motor geométrico y Acabado** se activan en P2 al acordar contratos. **Máquina/Post** entra en P3 después de definir capacidades verificables de FluidNC. **UI/UX** colabora en P4. **Desbaste** no se activa antes del primer prototipo. Aplicar **un solo parche en cada momento**, bajo el contrato de `WORKFLOW.md`; propiedad y revisión independiente en `CHAT_ROLES.md`.

**Correcciones previas al cierre P1 (auditoría `a10e0c0`):** evitar representación visual identidad silenciosa cuando falla una transformación; no invalidar la geometría por cambios solo de nombre/F/S; probar end-to-end un STL en pulgadas, escalas no uniformes, rotación/traslación/pivote, guardado, BVH, contacto, origen y vigencia. No introducir postprocesador hasta poder demostrar estas invariantes. Benchmark CAM grande con cifras reales, no estimadas.

## Criterios de calidad transversales

- Separación estricta entre **código CAM**, render, almacenamiento, post y seguridad de máquina.
- Conservación `f64` en coordenadas geométricas; rechazo de NaN/infinitos, desbordamientos y consultas fuera de rango.
- Resultados y pruebas vinculados a versión/entorno; no extrapolar de «cargo build» a precisión ni de un STL visual fluido a BVH CAM rápido.
- Desarrollo por **parches `.ps1` locales** con preflight, hashes e idempotencia; sin Git en chats. Debate y aceptación previos; documentación después de validar; commits exclusivos del usuario (D-036–D-044).
- Proyectos `.lagucam` mediante reemplazo seguro, **sin backups automáticos**; no tocar STL personales.

**Primera tarea al reanudar:** inspeccionar los archivos locales actuales y proponer únicamente el alcance de **P1**, con criterios de aceptación y pruebas. No implementar antes de la aprobación.

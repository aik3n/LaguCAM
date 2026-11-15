# LaguCAM — Instrucciones para asistentes y colaboradores

**Protocolo de colaboración vigente:** 2026-11-15. **Repositorio:** https://github.com/aik3n/LaguCAM.

> No tomar un SHA fijo de este archivo como estado actual. Antes de cada tarea, comprobar HEAD de GitHub y consultar `docs/STATUS.md`: distingue el último **código revisado** del último **código documentado**, que puede estar rezagado de forma deliberada.

## 1. Responsabilidades

**Responsable del proyecto (usuario):** decide prioridades y aprueba especificaciones, cambios, validaciones y publicación. Sus pruebas en equipo real se registran con su alcance; no sustituyen una validación geométrica ni de seguridad CNC.

**Chat de dirección técnica y documentación:** debate alternativas, revisa commits y evidencias, mantiene las decisiones técnicas y **actualiza los Markdown del proyecto una vez terminada la validación**, cuando el usuario lo solicita o dentro de una entrega documental autorizada. Solo redacta prompts de implementación cuando se le piden.

**Chat de programación:** lee el código real, implementa únicamente el cambio aprobado, ejecuta las pruebas disponibles y entrega SHA, cambios y evidencias. **No modifica documentación (`*.md`) por iniciativa propia**; prepara un resumen de hechos verificables para su actualización posterior. No amplía silenciosamente alcance o dependencias.

**Chat especializado en motor CAM:** investiga y desarrolla algoritmos geométricos dentro del alcance aprobado; no modifica documentación, interfaz, proyecto, bibliotecas ni postprocesadores sin autorización expresa.

**GitHub:** fuente de verdad para el **código versionado**. `docs/STATUS.md` es la fuente de verdad del **último estado documentado**, con un SHA base inequívoco. La validación se acredita con sus resultados, no con la mera presencia del código.

## 2. Ciclo de cinco fases (obligatorio)

1. **Debate:** delimitar problema, alternativas y riesgos; no generar código por iniciativa propia.
2. **Aceptación:** el usuario aprueba objetivo y alcance; el encargo de implementación explicita qué no debe cambiar.
3. **Implementación:** cambios acotados, commits identificables y referencia al SHA de partida; sin actualizar los Markdown.
4. **Validación:** ejecutar pruebas técnicas y, cuando corresponda, comprobación del usuario; entregar evidencia, fallos y limitaciones. No declarar aceptado un comportamiento no comprobado.
5. **Documentación:** este chat sincroniza `docs/STATUS.md` y únicamente los otros documentos afectados; registra el SHA del código validado y el nivel de evidencia. No es obligatorio documentar cada commit intermedio.

**La documentación puede estar temporalmente atrasada y eso no constituye por sí mismo un fallo.** Lo inadmisible es presentarla como vigente para un commit posterior al que cubre. Una nueva tarea debe verificar el código HEAD antes de asumir datos documentales.

Detalles del protocolo y entrega: `docs/WORKFLOW.md`.

## 3. Niveles de evidencia

- **IMPLEMENTADO EN CÓDIGO:** inspección del SHA y componentes correspondientes; no implica prueba de funcionamiento.
- **PRUEBAS EJECUTADAS:** comandos, resultado y SHA/entorno precisados; las referencias de terceros se atribuyen.
- **VALIDADO POR EL USUARIO:** conducta observada en su equipo, hardware y casos ensayados, sin extrapolación.
- **VALIDACIÓN CAM PENDIENTE:** aún faltan referencias independientes, tolerancias, comprobación del recorrido y seguridad.
- **FUTURO / PROPUESTO:** no implementado o no aprobado para desarrollo.

No llamar «seguro», «verificado» o «apto para mecanizado» a una previsualización o a un build correcto.

## 4. Contexto técnico estable

- CAM experimental libre, Rust 2024, egui/eframe, glow/OpenGL; tres ejes inicialmente, herramienta paralela al Z del setup.
- STL binario grande como formato inicial; cálculo `f64` sobre las coordenadas originales del archivo (decodificadas desde `f32`), nunca sobre la malla GPU normalizada.
- Biblioteca global de cinco familias de fresas. UUID = identidad; revisión = historial; SHA-256 geométrico = equivalencia de definición física. El proyecto conserva UUID, **no snapshots persistentes** de la herramienta.
- El primer postprocesador objetivo será FluidNC, sin acoplarlo al motor CAM. `G54` no implica rotación del marco y `M6` no garantiza cambio físico de herramienta.
- La documentación viva es experimental: **no se exigen migraciones de formatos durante desarrollo**; nunca sobrescribir ni eliminar datos personales silenciosamente.
- El cálculo de contacto, el muestreo experimental, una trayectoria CNC válida y un programa G-code seguro son niveles diferentes. No confundirlos.

Consulta el estado implementado **actualizado hasta un SHA concreto** en `docs/STATUS.md`, la arquitectura en `docs/ARCHITECTURE.md` y las decisiones en `docs/DECISIONS.md`.

## 5. Antes de tocar código

1. Comprobar rama, HEAD, árbol de trabajo y diferencias respecto de `docs/STATUS.md`.
2. Leer `docs/INDEX.md`, decisiones relevantes y archivos de código realmente afectados.
3. Identificar objetivo aprobado, exclusiones, pruebas y limitaciones de acceso.
4. No usar datos/documentos de una versión anterior como sustituto del código actual.

## 6. Entrega del chat de implementación

Informar, de forma breve: SHA base y SHA final (solo si hay commit), archivos alterados, pruebas **realmente** ejecutadas con entorno y resultados, pruebas pendientes, regresiones observadas, limitaciones y asuntos que requieren aceptación. **Sin modificar Markdown**.

Ante un cambio de alcance, seguridad, formatos persistentes o licencia, consultar antes de implementarlo. Nunca presentar una simulación como validación física. No publicar ni fusionar cambios por iniciativa propia.

**Regla de continuidad:** el repositorio manda sobre el código; `STATUS.md` manda sobre lo que la documentación afirma haber revisado; las pruebas mandan sobre lo que se considera validado.

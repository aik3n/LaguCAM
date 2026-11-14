# LaguCAM — Instrucciones para asistentes de IA

**Versión del documento:** 1.0 · **Fecha:** 2026-11-14  
**Repositorio:** https://github.com/aik3n/LaguCAM  
**Código de referencia actual:** `f1e436b976eb6f4f88482b00ae5aeff2761e08ee`

Este archivo orienta a cualquier chat o agente que participe en LaguCAM. Debe leerse junto a `docs/INDEX.md`, `docs/STATUS.md`, `docs/DECISIONS.md` y `docs/WORKFLOW.md`. Recoge el **protocolo vigente** de colaboración aprobado para la etapa experimental. No sustituye instrucciones de seguridad ni los requisitos de la tarea actual.

## 1. Dos funciones distintas

### Chat de dirección técnica (conversación de decisiones)

- Su función principal es **debatir, aclarar requisitos, comparar alternativas y decidir el siguiente paso con el usuario**.
- **No genera prompts, código, archivos, informes largos ni modificaciones por iniciativa propia.** Solo cuando el usuario lo solicite o la tarea acordada lo exija explícitamente.
- Responde de forma breve y precisa. No repite en cada respuesta todo el contexto ni propone una nueva pregunta cuando la decisión actual ya está cerrada.
- Expone diferencias entre lo decidido, una propuesta y una hipótesis. Si discrepa, justifica el riesgo sin convertirlo en una disputa.
- Ante una nueva idea, primero define implicaciones y alcance. **Debate antes de emitir la orden de desarrollo**.
- Puede revisar un commit o un fallo concreto cuando el usuario lo proporciona; debe verificar el SHA y limitar la revisión al motivo solicitado.

### Chat de desarrollo (preferentemente Work)

- Ejecuta **solo** el encargo aprobado: inspección del estado real, implementación incremental, pruebas, documentación y entrega.
- No decide silenciosamente nuevas funcionalidades, cambios de framework, ampliaciones de alcance ni refactorizaciones extensas.
- Consulta los documentos del proyecto, el código y el SHA antes de tocar archivos. Si difieren, informa y no inventa un estado conciliado.
- Mantiene el código compilable en cada entrega; usa commits pequeños e identificables cuando el entorno lo permite.
- No declara "compilado", "probado", "seguro" o "funciona" sin evidencia del entorno correspondiente.
- No puede probar realmente un escritorio/GPU o una fresadora solo porque el código compile. Indica qué ensayos quedan al usuario.

## 2. Comportamiento requerido en toda conversación

1. **No rellenar la respuesta.** Ir al punto, con el detalle necesario para la decisión o entrega, sin listas decorativas, reiteraciones ni elogios.
2. **No improvisar código ni prompts** mientras se está debatiendo un requisito. Esperar a que el usuario pida una instrucción de desarrollo.
3. **No preguntar lo que ya está decidido o documentado.** Si falta información esencial, preguntar solo lo necesario; si no bloquea, marcar el supuesto y seguir con alcance acotado.
4. **Separar los niveles de evidencia:**
   - **Probado por el usuario en su equipo**.
   - **Verificado mediante código o pruebas ejecutadas por el agente**.
   - **Afirmado por un informe anterior, sin verificación independiente**.
   - **Diseñado o pendiente**, no implementado.
5. **No confundir una interfaz visual con una garantía de mecanizado.** Nada de G-code operativo sin algoritmos, límites, comprobaciones y postprocesador validados.
6. **Mantener independencia de máquina, formato y GUI** en los componentes CAM siempre que resulte razonable. No inventar infraestructura genérica de 5 ejes cuando trabajamos en 3.
7. **No modificar silenciosamente archivos del usuario ni la rama principal.** Acciones de escritura, commits y publicación en GitHub requieren el encargo adecuado.
8. **No crear dependencias copyleft sin evaluación expresa de compatibilidad de licencias.** Revisar las licencias transitivas antes de integrar bibliotecas.
9. **Priorizar estabilidad y pruebas de regresión** sobre velocidad o cantidad de funcionalidades.
10. **Reconocer límites:** si no se dispone del STL real, GPU, controlador FluidNC o herramientas de compilación, dejarlo claro.

## 3. Contexto técnico de una nueva conversación

- Aplicación actual: Rust, `egui`/`eframe`, OpenGL mediante `glow`.
- Uso inicial: STL topográficos de QGIS/DEMto3D, algunos cercanos a 1 GB y decenas de millones de triángulos.
- El usuario ha logrado **abrir y mover fluidamente un STL real de casi 1 GB**. No hay benchmark cuantitativo archivado.
- Existen árbol lateral, bruto, origen CNC, persistencia `.lagucam` **v2**, biblioteca global de cinco geometrías de fresa, configuración portable, temas y docking de paneles. **Pendientes:** selección de herramientas por operación, cálculo CAM, máquina y postprocesadores.
- Tres ejes al principio, herramienta paralela a Z del sistema de preparación. Sin suponer que `G54` rota por sí mismo los ejes o que `M6` gestiona cambios automáticos en FluidNC.
- FluidNC es el **primer postprocesador objetivo**, no una dependencia del motor CAM. La máquina de pruebas cambia herramientas manualmente, otras máquinas podrán tener ATC.
- **UUID:** biblioteca, operación y `.lagucam` v2 usan `ToolId`. La biblioteca sigue siendo la única definición editable; no hay snapshots de fresa en el proyecto.
- **Huella SHA-256:** se han acordado los parámetros geométricos y una codificación canónica/versionada, pero la función aún **no está implementada**.
- **Compatibilidad en desarrollo:** no se exigen migraciones ni soporte de formatos antiguos. Se documentan cambios incompatibles y nunca se borran datos personales sin autorización.
- **No usar posiciones normalizadas de GPU (`f32`) para cálculo CAM**. Geometría y transformaciones de proyecto en `f64` según corresponda.

## 4. Inicio de tarea en el chat de desarrollo

Leer, en este orden: `docs/INDEX.md` → `docs/STATUS.md` → `docs/DECISIONS.md` → `docs/ARCHITECTURE.md` → documento de la tarea. Revisar `git status`, commit y código relevante. Si el SHA o los ficheros no coinciden con la documentación, informar antes de asumir conclusiones.

Responder al inicio con: **objetivo entendido, límites del cambio y posibles bloqueos reales**. No producir un plan excesivo para trabajos triviales.

## 5. Entrega mínima de cada cambio

- SHA de partida y SHA final **si se creó realmente un commit**.
- Archivos cambiados y motivo, en pocas líneas.
- Pruebas realizadas con comandos y resultados; distinguir pruebas no ejecutadas.
- Verificación de que el visor STL grande no se ha degradado, o indicación de que falta comprobarlo en el equipo real.
- Riesgos y puntos de revisión pendientes.
- Documentación actualizada en el mismo cambio, siguiendo `docs/WORKFLOW.md`.

**Principio rector:** el usuario dirige las prioridades; el chat de desarrollo implementa un alcance aprobado; la documentación refleja el código y las pruebas, nunca lo sustituye.

## 6. Convenciones adicionales para el chat de desarrollo

- Si la tarea es solo documental, no afirmar que se han ejecutado Rust, GUI o pruebas en la máquina.
- Tratar `LAGUCAM_004.md`–`LAGUCAM_007.md` y `AUDITORIA.md` como documentos históricos; verificar siempre HEAD y `docs/STATUS.md`.
- No convertir la decisión de SHA-256 en código sin encargo expreso. La próxima tarea acordada sería exclusivamente función de huella y tests; sin operaciones ni cambios adicionales de `.lagucam`.
- Separar **UUID (identidad)**, **revisión (historial)**, **hash geométrico (equivalencia de forma)** y **parámetros tecnológicos/G-code (vigencia aparte)**.

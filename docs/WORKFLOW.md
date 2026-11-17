# Protocolo de trabajo, parches PowerShell y trazabilidad

**Vigente:** 2026-11-17 · **Autoridad:** usuario · **Decisiones:** D-036–D-044. Roles en [`CHAT_ROLES.md`](CHAT_ROLES.md); sucesión en [`HANDOFF.md`](HANDOFF.md).

## 1. Cinco fases con barreras verificables

| Fase | Salida obligatoria | Quién autoriza la siguiente |
| --- | --- | --- |
| 1. **Debate** | Problema, evidencia de la base, alternativas, riesgos, contrato e invariantes | Usuario |
| 2. **Aceptación** | Alcance y exclusiones concretas, archivos permitidos, pruebas y criterios de cierre | Usuario, explícitamente |
| 3. **Implementación** | **Un `.ps1` autocontenido**, manifest de cambios, modo de precomprobación, comandos de ejecución y validación | Usuario ejecuta; ningún chat ejecuta Git |
| 4. **Validación** | Salidas reales de tests y GUI, incidencias, correcciones por parche, conformidad del Integrador y auditoría Master si aplica | Usuario acepta o rechaza |
| 5. **Documentación** | Parche `.ps1` documental que actualiza solo fuentes de autoridad pertinentes y `HANDOFF`; SHA posterior solo si se comunica | Usuario lo aplica y controla commit/push |

Ni debatir una mejora ni publicar un commit equivale a autorizar código nuevo. El Master puede **recomendar no aceptar** una entrega si faltan pruebas, pero no sustituye al usuario. No es obligatorio documentar cada microcambio: agrupar por hitos **validados**; una actualización explícita de reglas de gobierno como esta es una excepción autorizada.

## 2. Dónde se trabaja y quién usa Git

```text
C:\Users\<usuario>\Desktop\
├── LaguCAM\                     # Código y documentación reales
│   ├── Cargo.toml
│   ├── src\
│   └── docs\
└── parche_HITO_descripcion.ps1    # Archivo entregado por chat
```

Los chats pueden inspeccionar archivos locales si el usuario los facilita o la herramienta los expone, y GitHub en **lectura** para auditorías; **no ejecutan Git** (`status`, `diff`, `checkout`, `commit`, `push`, `merge`, etc.) y no editan localmente LaguCAM por otro mecanismo. El usuario ejecuta los scripts y todas las operaciones Git. El script debe localizar la carpeta con `$PSScriptRoot`, independientemente del directorio de trabajo; evitar rutas personales fijas.

**Un único parche pendiente de validación cada vez.** Se pueden estudiar varios contratos en paralelo, pero las aplicaciones son secuenciales. No generar sobre una base imaginada por otro chat: usar el estado local real y precondiciones por archivo.

## 3. Especificación de entrega de un parche

Cada entrega debe contener, de forma verificable:

1. **Identificación:** ID de hito, objetivo, exclusiones, rol responsable, base pública de referencia solo si se conoce; el SHA GitHub no sustituye la base local.
2. **Manifest de archivo:** ruta relativa bajo `LaguCAM`, operación (crear/modificar/borrar autorizado), hash de contenido previo esperado y hash del nuevo contenido. Para texto UTF-8, normalizar LF/CRLF y BOM de modo explícito; declarar otros encodings. Nunca basarse únicamente en coincidencia de una subcadena.
3. **Payload completo:** cambios y datos **dentro de un solo `.ps1`**. Nada de descargar archivos, ejecutar scripts remotos, instrucciones manuales de copiar fuentes o modificar otros directorios.
4. **Precomprobación de toda la transacción:** comprobar estructura de proyecto, `Cargo.toml`, todas las rutas, archivos esperados, hashes, ausencias de nuevos archivos, permisos y que ninguna ruta escape al árbol de trabajo. Rechazar enlaces simbólicos/junctions/reparse points y nombres con recorrido `..` no previstos. Si hay **cualquier conflicto**, abortar antes de escribir.
5. **Aplicación controlada:** preparar y verificar el contenido de **todos** los destinos antes de reemplazar. Temporales en el mismo volumen/directorio; sustitución segura **por archivo**. No usar `[IO.File]::Replace()` con ruta de backup vacía o inválida: ese método puede fallar en Windows; emplear ruta temporal de rollback válida o API probada. Sin `-Force` ni silenciamiento de fallos sin una precondición demostrable.
6. **Recuperación:** no prometer atomicidad de un conjunto de archivos. Si falla un paso, intentar revertir los ya cambiados a su contenido inicial; conservar e informar cualquier temporal de recuperación que no se pueda limpiar. Tras interrupción eléctrica, una reejecución debe identificar archivos **base**, **ya aplicados** o **conflictivos**; nunca reemplazar conflictos. No retirar archivos originales hasta terminar las comprobaciones.
7. **Idempotencia y trazabilidad:** modo de **comprobar sin modificar**, resultado legible por archivo y mensaje final inequívoco; si todo coincide con destino, indicar «ya aplicado». El script no crea commits ni cambia `.git`.
8. **Superficie de seguridad:** no modificar `.lagucam` personales, STL, bibliotecas reales, configuraciones externas o políticas persistentes salvo alcance autorizado; no red, no `Invoke-Expression`, no autoelevación ni procedimientos destructivos. No crear backups de proyectos `.lagucam` por cada guardado (D-039). **Rollback temporal de código/documentación** es distinto: se elimina al terminar o se informa si queda tras un fallo.

**Para datos binarios o archivos muy grandes**, no encajar silenciosamente el parche textual: acordar un formato verificable y aprobar sus salvaguardas. Un parche de documentación solo modifica `.md`/`README.md` previamente enumerados.

## 4. Validación del propio parche antes de entregarlo

El chat responsable comprueba la coherencia del contenido y los hashes contra los archivos de referencia accesibles. Siempre que pueda, prueba sobre una **copia temporal** del estado base (nunca el proyecto personal): precomprobación, primera aplicación, segunda aplicación sin cambios y rechazo de un archivo modificado. Para cambios multarchivo, comprobar reversión ante fallo simulado. **Si no pudo ejecutar PowerShell 5.1 o probarlo en Windows, declararlo; las pruebas estáticas no lo sustituyen.**

El usuario ejecuta desde PowerShell:

```powershell
# 1. Precomprobar; debe NO modificar el proyecto
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:USERPROFILE\Desktop\parche_HITO_descripcion.ps1" -CheckOnly

# 2. Aplicar una sola vez tras revisar el resumen
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:USERPROFILE\Desktop\parche_HITO_descripcion.ps1"
```

Si falla: **detenerse y enviar la salida completa** al chat responsable. No forzar, corregir a mano, cambiar hashes ni ejecutar un segundo parche sobre el estado dudoso. El responsable audita el punto de fallo y entrega un parche correctivo específico, si procede.

## 5. Calidad de Rust y pruebas funcionales

El parche de código adjunta los comandos **necesarios para su riesgo**, no un listado inerte. Comandos de referencia desde la carpeta real:

```powershell
Set-Location "$env:USERPROFILE\Desktop\LaguCAM"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo check --locked --all-targets
cargo build --locked --release
```

Exigir también tests unitarios/integración específicos, GUI y patrones geométricos según el contrato. Registrar comandos, resultado real, Windows/Rust/driver cuando afecte, rendimiento medido, errores y regresiones. `cargo fmt` sin `--check` modifica archivos: no pedirlo como validación pasiva. **Un cambio exclusivamente documental no necesita Cargo.**

**Seguridad CNC:** un build correcto, contacto puntual, validación estructural o G-code exportado no acredita trayectoria segura; comprobar error entre muestras, volumen barrido, mango/portaherramientas, bruto, utillaje, límites, máquina y post. Simulación externa y prueba en vacío son controles adicionales, no garantías universales.

## 6. Informe mínimo al terminar un hito

```text
ID de hito / responsable / alcance aceptado:
Base local observada y SHA público de referencia (no asumir coincidencia):
Parche, versión y manifest de archivos:
Precomprobación y aplicación: comando, resultado, reintentos y temporales restantes:
Tests Windows: comando, SO/Rust, resultado exacto:
Pruebas GUI, geométricas, benchmark o máquina: casos y evidencia:
Defectos demostrados / riesgos / limitaciones / regresiones:
Revisión de contrato por Integrador; auditoría Master (si aplica):
Decisión del usuario: aceptado / corrección / pendiente:
Parche documental ejecutado: sí/no, evidencia:
SHA posterior comunicado por usuario (si existe):
Próxima decisión y dueño:
```

## 7. Relación entre documentos, parches y relevo

- **Archivos locales**: verdad para el siguiente parche; si se desconocen, no se finge haberlos visto.
- **GitHub público**: verdad de la versión publicada inspeccionada; no da estado local.
- **`STATUS.md`**: único estado técnico documental, pruebas y SHA cubierto.
- **`DECISIONS.md`**: acuerdos vigentes y su historial; nada queda aceptado automáticamente por mencionar una idea.
- **`CHAT_ROLES.md`**: fronteras entre chats; **`HANDOFF.md`**: relevo operativo del Master, sin duplicar todo `STATUS`.
- **`ROADMAP.md`**: prioridades P1–P4 y bloqueos; no equivale a autorización de tareas.

Tras validar un hito, el Master prepara documentación y actualiza `HANDOFF`. Si hay sucesión, **el usuario designa al siguiente Master** y este reconcilia documentos, código accesible y evidencia antes de reordenar tareas. Los chats son sustituibles; los contratos y decisiones versionadas, no su memoria, preservan continuidad.

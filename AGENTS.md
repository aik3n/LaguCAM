# LaguCAM — Instrucciones para asistentes y colaboradores

**Protocolo actualizado:** 2026-11-17 · **Proyecto:** LaguCAM · **Repositorio de referencia:** https://github.com/aik3n/LaguCAM.

> **Lugar de trabajo para implementar: la carpeta local `Escritorio/LaguCAM`.** Los chats no realizan operaciones Git. El responsable del proyecto controla los commits y la publicación. Para el último corte técnico documentado consultar [`docs/STATUS.md`](docs/STATUS.md); puede diferir de los archivos locales mientras se aplica un parche o de GitHub tras un commit.

## 1. Autoridad y responsabilidades

- **Responsable del proyecto (usuario):** decide prioridades, acepta cada alcance, ejecuta los parches y las pruebas en su Windows, valida la aplicación y realiza personalmente todas las operaciones Git (commits, ramas, merges, push). No se presupone un commit ni se solicita al chat que lo cree.
- **Chat de dirección técnica/documentación:** debate, audita código y evidencias, prepara instrucciones y un **parche documental `.ps1`** cuando se autoriza la fase documental. Nunca declara cambios locales aplicados si el usuario no ha ejecutado el parche.
- **Chat de programación:** inspecciona los archivos **locales** proporcionados/accesibles, implementa el alcance autorizado y entrega **un único parche `.ps1` por avance**, junto con comandos de aplicación, compilación y tests. No usa Git y no edita `*.md` por iniciativa propia.
- **Chat de motor CAM:** las mismas restricciones de parche, Git y documentación; limita los cambios a los algoritmos autorizados. No altera interfaz, persistencia ni post sin consentimiento.
- **GitHub:** referencia del código **ya versionado** y fuente para auditorías remotas; no sustituye el estado real de archivos locales antes de preparar/aplicar parches. El usuario podrá comunicar el SHA publicado tras el commit.

## 2. Ciclo obligatorio de cinco fases (D-036)

1. **Debate:** problema, alternativas, riesgos, dependencias y criterios de prueba; ninguna idea supone aprobación automática.
2. **Aceptación:** el responsable autoriza expresamente el alcance; si requiere implementación, se determina qué archivos y contratos pueden cambiar.
3. **Implementación por parche:** el chat entrega el `.ps1` completo, **sin ejecutarlo en el equipo del usuario**, y las instrucciones necesarias. No toca Markdown salvo encargo documental explícito.
4. **Validación:** el usuario ejecuta el parche, compila y pasa los tests indicados, comprueba la GUI cuando proceda y comunica errores/resultados. Los fallos se corrigen con un nuevo parche controlado, no mediante un commit automático.
5. **Documentación:** al aceptar las pruebas, se prepara/aplica un parche documental que registra el alcance y la evidencia concreta. **El commit/push posterior es exclusivamente del usuario.**

No es obligatorio documentar cada pequeño cambio intermedio. `docs/STATUS.md` indica exactamente el SHA de código versionado cubierto y, cuando proceda, la base local observada; una diferencia temporal no es un error si está explicada.

## 3. Contrato de entrega del parche `.ps1` (D-037 y D-038)

Disposición obligatoria de archivos:

```text
Escritorio/
├── LaguCAM/                   # Carpeta de proyecto existente
└── parche_nombre_hito.ps1    # Entregado por el chat
```

- El usuario guarda el script **junto a** `LaguCAM`, no dentro; el script localiza el proyecto a partir de `$PSScriptRoot` (no depende del directorio actual ni de rutas de otro usuario).
- Antes de escribir, comprobar carpeta, archivos esperados, compatibilidad del estado de partida y destino de los cambios. **Rechazar conflictos y fallos sin sobrescribir silenciosamente.** Las escrituras serán atómicas por archivo o tan seguras como permita el cambio.
- Cada parche debe ser específico, legible, de alcance limitado e idealmente reejecutable sin duplicar cambios. No ejecutar `git`, no modificar ramas ni publicar nada; no realizar descargas inesperadas ni cambios externos al proyecto.
- No crear backups automáticos de proyectos `.lagucam` ni archivos `.recovery.*.bak` (D-039). Evitar también manipular archivos STL personales; no confundir los temporales internos del parche con copias de seguridad persistentes.
- Entregar **comando PowerShell de aplicación**, **comandos concretos para `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test`, `cargo build --release` según el riesgo**, y pasos de prueba manual si hay cambios visuales/CAM. Si es exclusivamente documental, indicarlo y no atribuir tests Rust innecesarios.
- El chat informa de **qué se modificará**; el usuario confirma después qué se ejecutó. El chat no promete aplicación, compilación o validación en Windows sin evidencia.

Comando habitual desde PowerShell (usando la ubicación real del script):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\<usuario>\Desktop\parche_nombre_hito.ps1"
```

## 4. Evidencia y estado

- **INSPECCIONADO:** código, archivos o parche revisados; no equivale a ejecución.
- **PRUEBAS REPORTADAS:** comando, versión local o SHA identificado, Windows/entorno, resultado y ejecutor.
- **VALIDADO POR EL USUARIO:** comportamiento observado en hardware y caso concreto, sin generalizar.
- **VALIDACIÓN CAM PENDIENTE:** faltan tolerancias independientes, barrido continuo, colisiones, simulación o condiciones de máquina.
- **PROPUESTO / FUTURO:** no implementado o aún sin autorización.

No llamar «seguro», «verificado para CNC» o «mecanizable» al contacto por puntos, una vista previa, un `cargo build`, una validación estructural o un archivo G-code experimental.

## 5. Contexto técnico e invariantes

- Rust 2024; egui/eframe + glow/OpenGL. STL binario original (vértices `f32`) y operaciones geométricas en `f64`; la malla GPU normalizada no sirve para CAM.
- Tres ejes, herramienta paralela al Z del modelo/setup admitido. Las unidades del STL no están codificadas: el TEST actual supone milímetros; no convertirlo en supuesto implícito para producción.
- Biblioteca de herramientas global: UUID identifica herramienta; SHA-256 canónico identifica geometría física. No se guardan snapshots editables de herramienta en la operación. No basta el hash para validar toda la trayectoria.
- ContactSolver experimental: esférica, plana y toroidal. Resultado sin contacto (`None`) nunca es un movimiento seguro. Falta verificación de mango, portaherramientas, fijaciones y volumen barrido.
- Trayectoria neutral, enlaces propuestos y validador **estructural** existen en código; todavía no hay verificación integral de colisiones, biblioteca de máquinas ni postprocesador FluidNC operativo.
- Durante desarrollo pueden cambiar esquemas sin migración obligatoria, pero no se eliminan datos personales ni archivos del usuario silenciosamente.

## 6. Antes de cada trabajo y al entregarlo

1. Consultar archivos reales de la carpeta local; si solo se dispone de GitHub, declarar que se revisa una versión **remota** y no afirmar identidad con la carpeta local.
2. Leer `docs/INDEX.md`, `docs/STATUS.md`, `docs/DECISIONS.md` y las fuentes afectadas. Pedir lo indispensable si el chat no puede ver el estado local.
3. Fijar alcance autorizado, archivos afectados, precondiciones, comportamiento esperado, exclusiones y pruebas.
4. Entregar el `.ps1` y comandos de ejecución/validación. **No usar Git**. Después de las pruebas, dejar constancia de resultados reportados y pendientes.
5. En la fase documental, actualizar `STATUS.md` y los documentos que correspondan; no declarar SHA futuro hasta que el usuario lo comunique.

Más detalle: [`docs/WORKFLOW.md`](docs/WORKFLOW.md), [`docs/TESTING.md`](docs/TESTING.md) y [`docs/ROADMAP.md`](docs/ROADMAP.md).

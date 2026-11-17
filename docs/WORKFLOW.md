# Flujo de trabajo, parches y sincronización documental de LaguCAM

**Vigente:** 2026-11-17 · **Autoridad:** responsable del proyecto · **Decisiones:** D-036–D-039.

## 1. Las cinco fases

| Fase | Responsable y salida |
| --- | --- |
| **1. Debate** | Usuario y chat delimitan objetivo, alternativas, riesgos y criterios de aceptación. |
| **2. Aceptación** | El usuario aprueba expresamente el alcance; el chat no implementa propuestas no aprobadas. |
| **3. Implementación** | Chat entrega **parche `.ps1`** para la carpeta local y comandos de ejecución; no modifica `*.md` por iniciativa propia ni utiliza Git. |
| **4. Validación** | Usuario ejecuta parche y pruebas Windows, comprueba la aplicación y reporta resultados. Una prueba fallida bloquea el cierre del hito. |
| **5. Documentación** | Chat documental entrega el parche documental, lo aplica el usuario y registra evidencias y limitaciones. El usuario decide si y cuándo crea el commit. |

El chat no hace `git status`, `git checkout`, `git commit`, `git merge`, `git push` ni ninguna operación Git como parte de su trabajo. Puede utilizar SHA **comunicados por el usuario** o revisar GitHub para una auditoría del código versionado, pero la fuente de la modificación es la **carpeta local**.

## 2. Paquete de entrega de cada mejora

```text
C:\Users\<usuario>\Desktop\
├── LaguCAM\
│   ├── Cargo.toml
│   ├── src\
│   └── docs\
└── parche_nombre_hito.ps1
```

El parche se ejecuta **desde el Escritorio, al mismo nivel que `LaguCAM`**. Usa `$PSScriptRoot` para encontrar `LaguCAM`, de modo que funciona independientemente del directorio de trabajo de PowerShell.

Requisitos de calidad del script:

1. Verificar previamente estructura y archivos de partida; abortar si hay conflictos, rutas ausentes o versiones inesperadas.
2. Modificar solo archivos aprobados. Evitar efectos parciales; usar ficheros temporales y reemplazo seguro por archivo cuando proceda. No descargar recursos ni tocar configuraciones ajenas sin autorización.
3. Ser idempotente cuando sea posible: si ya se aplicó, informarlo en vez de duplicar secciones o alterar otra vez el código.
4. No generar copias automáticas `.bak`/`.recovery.*.bak` de proyectos `.lagucam` (D-039). Proteger STL y archivos personales.
5. Mostrar el resultado y el listado de archivos modificados. Ningún mensaje del script equivale a un commit.

El chat entrega junto al `.ps1` los comandos necesarios (aplicación, formato, check/clippy, tests y Release según el alcance) y los pasos de comprobación manual. No sustituir las pruebas reales por frases genéricas. Un cambio solo documental no requiere recompilar Rust salvo que se justifique expresamente.

Ejemplo de aplicación cuando el script está realmente en el Escritorio:

```powershell
& "$env:USERPROFILE\Desktop\parche_nombre_hito.ps1"
```

Si la política de ejecución lo impide, ejecutar desde PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:USERPROFILE\Desktop\parche_nombre_hito.ps1"
```

Ejemplo de comprobación de código Rust, **desde `LaguCAM`**:

```powershell
cd "$env:USERPROFILE\Desktop\LaguCAM"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo check --locked --all-targets
cargo build --locked --release
```

La lista se adapta al cambio. El usuario ejecuta los comandos en su Windows; el chat distingue pruebas hechas de comandos únicamente propuestos.

## 3. Estado local, commits y documentación

- **Archivos locales:** verdad para preparar y probar un parche. Pueden incluir cambios sin commit.
- **GitHub `main`:** código publicado, útil para auditoría o recuperar una base conocida; puede no coincidir con el escritorio.
- **`docs/STATUS.md`:** último corte documental con SHA de código inspeccionado, plataforma, pruebas **reportadas** y pendientes. No representa automáticamente el estado local o remoto posterior.
- **Commits:** todos los realiza el usuario. Tras hacerlo, puede facilitar el nuevo SHA para el siguiente corte. No se inventan SHAs de parches todavía no versionados.

La documentación puede agruparse por hitos y quedar rezagada mientras se valida. No se altera el SHA de código cubierto solo por preparar un parche o recibir un resultado de un commit distinto.

## 4. Documentos y responsabilidades

| Documento | Contenido |
| --- | --- |
| `AGENTS.md` y `WORKFLOW.md` | Reglas estables y entrega mediante `.ps1` |
| `docs/STATUS.md` | Implementación observada, evidencia y límites por SHA |
| `docs/DECISIONS.md` | Decisiones aceptadas, propuestas y sustituidas |
| `docs/ARCHITECTURE.md` | Contratos, módulos y responsabilidades |
| `docs/REQUIREMENTS.md` | Requisitos y criterios de aceptación |
| `docs/ROADMAP.md` | Secuencia de hitos hacia el prototipo |
| `docs/TESTING.md` | Comandos, pruebas, benchmarks y seguridad |
| `docs/LIBRARIES.md` | Herramientas, máquinas y postprocesadores |
| `docs/CHANGELOG.md` | Cambios de entregas; no reemplaza `STATUS.md` |

Historiales y documentos de una etapa antigua pueden conservarse como referencia fechada; señalar cuándo ya no describen la versión actual.

## 5. Informe mínimo al cerrar un avance

```text
Objetivo y alcance aceptados:
Base local / SHA público de referencia, si se conoce:
Nombre del parche .ps1 y archivos modificados:
Comando de aplicación y resultado reportado por el usuario:
Comandos de compilación/tests + Windows/Rust + resultado real:
Comprobación GUI y caso de prueba:
Regresiones y limitaciones:
Datos/esquemas afectados:
¿Fase documental aplicada?:
SHA de commit posterior (solo si el usuario lo aporta):
```

No escribir «pruebas superadas» cuando solo se ha preparado el comando. La ausencia de evidencia no invalida automáticamente la implementación, pero impide afirmar que esté validada.

## 6. Seguridad y calidad CAM

Compilación, validación estructural, contacto geométrico, ausencia de colisiones y aptitud para ejecutar una CNC son **niveles distintos**. Los enlaces de despeje y las alturas propuestas no prueban que el mango, fijaciones, máquina o volumen barrido sean seguros. La exportación FluidNC será un hito separado y estará bloqueada o claramente limitada hasta cumplir condiciones verificables.

## 7. Reanudación desde un nuevo chat

Indicar que LaguCAM se trabaja **localmente por parches `.ps1`**, sin operaciones Git del chat. Consultar `AGENTS.md`, `docs/STATUS.md`, `docs/ROADMAP.md`, `docs/DECISIONS.md` y los archivos locales reales. Si solo puede accederse al repositorio remoto, declararlo antes de proponer cambios que dependan de suponer identidad con el Escritorio. **No implementar sin aceptación del usuario**.

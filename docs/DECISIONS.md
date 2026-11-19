# Registro de decisiones técnicas

<!-- LAGUCAM-MASTER-DOC-CUT: 85dfda28658d5f8e779caae97838742d1444be73 -->

**Última actualización de decisiones:** 2026-11-18 · **Código público inspeccionado:** `85dfda28658d5f8e779caae97838742d1444be73` · **Evidencia:** [`STATUS.md`](STATUS.md)

Una decisión se cambia mediante una nueva entrada con justificación, no reescribiendo silenciosamente lo que se acordó. Las entradas **PROPUESTAS** no autorizan implementación.

| ID | Estado | Decisión | Justificación y consecuencia |
| --- | --- | --- | --- |
| D-001 | **ACEPTADA** | LaguCAM será software libre de CAM genérico; STL como primer formato | Evita encerrar el proyecto en topografía; diseño para STEP y otras geometrías más adelante |
| D-002 | **ACEPTADA** | Aplicación propia en Rust y egui; OpenGL/glow actualmente | No depender del rendimiento/importador de SolidCraft; no imponer cambio de stack hoy |
| D-003 | **ACEPTADA** | Tres ejes iniciales; 4/5 ejes solo como posibilidad futura | Controla complejidad y permite validar un flujo real |
| D-004 | **ACEPTADA** | Herramienta paralela a Z del setup; plano XY; descenso usual hacia −Z | No crear selector de dirección de husillo por operación 3 ejes. Z del STL puede diferir del setup, sujeto a transformación física/control válidos |
| D-005 | **ACEPTADA** | Separar malla original, representaciones auxiliares y malla de GPU | No usar visualización aproximada para trayectorias CNC |
| D-006 | **ACEPTADA** | Algoritmos directos y auxiliares optimizados posibles, con tolerancia y verificación | Elegir tras comparar precisión y rendimiento, no por preferencia tecnológica |
| D-007 | **ACEPTADA** | Soporte geométrico por capacidades, no solo formato/extension | Permite una estrategia con distintos backends, o restringirla cuando no existe geometría suficiente |
| D-008 | **ACEPTADA** | Biblioteca global y permanente de fresas, con filtros en vez de carpetas obligatorias | Selección rápida desde operaciones; no depender de estructura manual de carpetas |
| D-009 | **ACEPTADA** | Separar geometría de herramienta y parámetros de corte/uso | Evita acoplar cinemática/colisión a RPM y feed; facilita reutilización |
| D-010 | **SUSTITUIDA** | Copia de definición de herramienta dentro de la operación | Sustituida por D-024: referencia al UUID global, sin snapshot persistente de geometría |
| D-011 | **ACEPTADA** | Bibliotecas independientes de herramientas, máquinas y postprocesadores | El motor CAM no depende de FluidNC ni del modo de cambio de herramienta |
| D-012 | **ACEPTADA** | Primer postprocesador: FluidNC | Permite validar una máquina real, sin bloquear futuros controles |
| D-013 | **ACEPTADA** | Modo de cambio manual/automático pertenece a capacidades de máquina/control | La máquina actual cambia fresas a mano; otras pueden tener ATC |
| D-014 | **ACEPTADA** | No fijar T1/T2 a la geometría de la fresa | Asignación en configuración de máquina/trabajo |
| D-015 | **ACEPTADA** | Proyecto separado del STL externo; archivo con esquema versionado | Evitar duplicar gigabytes y permitir evolución del formato |
| D-016 | **ACEPTADA; PRECISADA POR D-037** | Dirección técnica y desarrollo en chats separados; GitHub registra código versionado | Los chats trabajan sobre archivos **locales** y no realizan operaciones Git; el usuario gestiona commits y publicación |
| D-017 | **ACEPTADA; PRECISADA POR D-038** | Documentación viva por hitos validados, mediante parches locales | Poder reanudar otro chat con evidencia; la publicación Git corresponde solo al usuario |
| D-018 | **ACEPTADA** | Para cambios de setup, no asumir que G54 efectúa rotación | G54 selecciona offsets; rotaciones requieren soporte/configuración explícitos del control |
| D-019 | **ACEPTADA** | No asumir que M6 ejecuta cambio manual en FluidNC | La secuencia exacta debe validarse con la configuración de la máquina |
| D-020 | **PROPUESTA** | Cinco tipos iniciales: plana, esférica, V, cónica plana y cónica esférica | Referencia DeskProto; por cerrar tras definir geometrías y fórmulas |
| D-021 | **ACEPTADA PARA PROTOTIPO TEST** | Primera estrategia: acabado paralelo con fresa esférica | Muestreo y previsualización experimental, **no** aprobación de mecanizado CNC |
| D-022 | **PROPUESTA** | Un perfil de máquina por proyecto, configurable | Simplificación inicial; el usuario no ha confirmado el modelo definitivo |
| D-023 | **ACEPTADA; IMPLEMENTADA EN CÓDIGO** | `.lagucam` JSON versionado | Esquema actual v2; no se exige compatibilidad con v1 durante desarrollo (D-033) |
| D-024 | **ACEPTADA** | Herramienta definida solo en biblioteca global; operación guarda UUID sin copia de geometría | La falta de referencia bloquea futuro cálculo; comprobar vigencia antes de reutilizar trayectorias |
| D-025 | **ACEPTADA** | Navegación visual independiente del formato, con picking de profundidad renderizada | El visor elige pivote sin releer STL ni usar geometría CAM |
| D-026 | **ACEPTADA** | Selección visual del pivote por clic simple, sin mover la cámara en el clic | Un clic sobre el fondo conserva el pivote |
| D-027 | **ACEPTADA** | El gesto pulsar-y-arrastrar fija primero el pivote y después orbita | Evita rotar inicialmente sobre el pivote anterior |
| D-028 | **ACEPTADA** | Marcador de pivote visible solo mientras se orbita; acciones de ratón centralizadas | No confundir pivote persistente con indicador temporal |
| D-029 | **ACEPTADA** | Proyecto y Propiedades son paneles independientes, flotantes o acoplados | Estado visual global, no parte del `.lagucam` |
| D-030 | **ACEPTADA** | Arrastre, guías visuales y acoplamiento izquierda/derecha, arriba/abajo | Mantener tamaños independientes y `panels.json` |
| D-031 | **ACEPTADA** | Distribución portable y temas Claro/Oscuro | `config/` junto al ejecutable; sin AppData ni Registro |
| D-032 | **ACEPTADA** | Docking con asa de seis puntos, silueta y guías sin texto | Sustituye menú «Posición», sin cambios en los datos de proyecto |
| D-033 | **ACEPTADA** | Durante desarrollo no se exige compatibilidad de formatos ni migraciones | Documentar roturas y conservar datos personales; `.lagucam` v1 se rechaza en el SHA revisado |
| D-034 | **ACEPTADA; FUNCIÓN IMPLEMENTADA** | SHA-256 geométrico binario canónico y versionado | Igual geometría → mismo hash entre revisiones; aún no valida trayectorias |
| D-035 | **ACEPTADA PARA PROTOTIPO** | Crear operación vacía y editarla en ventana única con pestañas, abriéndola con doble clic | UI experimental incorporada en `7de83c9` y `13a439a`; no genera G-code |
| D-036 | **ACEPTADA; PROCESO VIGENTE** | Cinco fases: debate → aceptación → implementación → validación → documentación | La documentación se prepara tras validar; ejecución de parches y commit final son del usuario (D-037/D-038) |
| D-037 | **ACEPTADA (2026-11-17)** | Los chats no usan Git; trabajan sobre carpeta local `Escritorio/LaguCAM` | Único responsable de branches, commits, merges y push es el usuario. Un SHA remoto no sustituye el estado local sin comprobar |
| D-038 | **ACEPTADA (2026-11-17)** | Todo avance se entrega como parche PowerShell `.ps1` situado junto a la carpeta de proyecto, con ejecución, compilación y tests | El script usa `$PSScriptRoot`, verifica la base y evita sobrescrituras inesperadas; lo aplica/prueba el usuario. Documentación solo en fase autorizada |
| D-039 | **ACEPTADA (2026-11-17)** | Guardado de `.lagucam` por reemplazo sin copias `.recovery.*.bak` automáticas | No guardar backups de cada sobrescritura; mantener manejo explícito de errores y respeto a los archivos personales |
| D-040 | **ACEPTADA (2026-11-17)** | Master sustituible, sin autoridad unilateral de implementación; `docs/HANDOFF.md` como relevo operativo | Usuario designa Master; se transmiten decisiones, evidencia y siguiente hito sin depender de memoria de chats |
| D-041 | **ACEPTADA (2026-11-17)** | Integrador CAM único responsable del flujo completo; especialistas por contratos, desbaste posterior | Priorizar integración P1–P4, evitar dispersión y solapes entre chats; propiedad de archivos por hito en `CHAT_ROLES.md` |
| D-042 | **ACEPTADA (2026-11-17)** | Parches `.ps1` autocontenidos, con preflight por hashes, modo `-CheckOnly`, idempotencia, staging y recuperación | Ningún chat ejecuta Git ni toca la carpeta local directamente; no afirmar atomicidad multarchivo; gestionar conflictos sin sobrescribir |
| D-043 | **ACEPTADA (2026-11-17)** | Auditoría crítica e independiente, evidencias etiquetadas, un único parche en aplicación/validación | No aprobar por ausencia de avisos ni por compilar; priorizar riesgos y corrección geométrica frente a estética o refactorización masiva |
| D-044 | **ACEPTADA (2026-11-17)** | Refactorización selectiva por problema demostrable y pruebas, no por longitud de archivo | Mantener contratos estables, limitar alcance y extraer responsabilidades solo cuando facilita el hito o reduce riesgo probado |
| D-045 | **ACEPTADA (2026-11-18)** | La estrategia comienza en el primer XY de mecanizado a su Z de seguridad sobre el bruto | No mover XY por cuenta propia a zonas seguras de máquina; sí conservar enlaces internos entre pasadas a despeje definido |
| D-046 | **ACEPTADA (2026-11-18)** | Posición inicial física corresponde al usuario; el post puede preparar retirada vertical inicial con G53 configurable por máquina | Homing y posición MPos segura verificados; no fijar Z universal, no inventar estado de máquina; G53 aún no interpretado en el código |
| D-047 | **PROPUESTA (2026-11-18)** | Planificador externo a las estrategias coordinará transiciones entre operaciones | El cálculo conjunto actual no genera un NC conjunto ni certifica transiciones XY/Z, cambios de herramienta o colisiones |


## Cómo registrar una nueva decisión

Cada nueva entrada debe contener ID, fecha, estado (**PROPUESTA**, **ACEPTADA**, **SUSTITUIDA**, **DESCARTADA**), contexto del problema, alternativas consideradas, criterio elegido, impacto, pruebas necesarias y SHA si produjo código.

No convertir automáticamente una recomendación de un asistente en **ACEPTADA** sin respuesta del usuario. Si se cambia una decisión, enlazar la entrada a la decisión anterior y explicar las migraciones/repercusiones.

**Normativa de aplicación de D-040–D-044:** [`CHAT_ROLES.md`](CHAT_ROLES.md) define responsables y secuencia; [`WORKFLOW.md`](WORKFLOW.md) regula aceptación, scripts, pruebas y recuperación; [`HANDOFF.md`](HANDOFF.md) facilita la sustitución del Chat Master. Los cambios de gobernanza **no acreditan pruebas Rust**.

## D-045 — Contrato de inicio y límite de responsabilidad de estrategia (ACEPTADA)

- **Fecha:** 2026-11-18; criterio expresamente indicado por el responsable.
- **Entrada de la trayectoria neutral:** primer punto del primer segmento al XY efectivo inicial de mecanizado, **Z en el despeje definido por la estrategia respecto al bruto**, no en una posición de máquina arbitraria. La misma estrategia resuelve aproximaciones, retiradas y enlaces de pasadas.
- **Prohibición:** no introducir por cuenta propia desplazamientos XY exteriores al trabajo para buscar una posición de máquina, aparcamiento u homing. Los enlaces XY internos entre pasadas a clearance siguen siendo movimientos de la estrategia.
- **Alcance de código:** `src/toolpath_links.rs` ya arranca con `safe_start` en el primer XY y clearance; comprobar el invariante con tests. No equivale a certificación de la trayectoria.

## D-046 — Preparación inicial mediante post/perfil de máquina (ACEPTADA COMO REPARTO)

- **Fecha:** 2026-11-18. El operador es responsable del homing, la posición física inicial, la herramienta, la fijación y el origen de trabajo; LaguCAM no puede inferirlos del primer movimiento CAM.
- **Opción de diseño aceptada:** post/perfil puede emitir retirada **solo Z** en coordenadas de máquina (`G53 G0 Z...`) a un valor **configurado y comprobado**, antes del posicionamiento XY de la operación. No se impone por defecto ni se presupone `G53 Z0` seguro.
- **Condiciones:** homing válido, MPos Z y límites reales de máquina conocidos, clearance suficiente para XY y estado modal del control. El post no inventa geometría de corte; preparaciones de máquina son responsabilidad externa a la estrategia.
- **Estado de implementación en `85dfda2`:** NO implementado como contrato verificado. `nc_interpreter.rs` rechaza G53; Start personalizado puede bloquear resumen/simulación automática; no declarar verificable el preámbulo.

## D-047 — Planificador global de transiciones (PROPUESTA)

- **Razon:** al encadenar operaciones habrá transiciones de Z/XY, cambios de herramienta y estado de control que no pertenecen a una estrategia individual.
- **Estado:** propuesta arquitectónica, no autorizacion de implementación. Diseñar contrato y pruebas con usuario, Integrador y Post antes de habilitar NC conjunto.
- **Distincion vigente:** `cam_batch.rs` calcula operaciones activas de forma secuencial; no planifica trayectos de máquina entre ellas ni genera un solo NC.

## Preguntas abiertas

- Los cinco tipos tienen convenciones implementadas (ángulo cónico **total** y radio de esfera definido). Antes de CAM deben verificarse matemáticamente para contacto, no solo dibujo.
- La toroidal ya está en código (G24); «ball with flat tip» sería otra geometría y queda fuera del primer cierre. No confundir implementación con validación independiente.
- Política de colocación de herramienta y offsets de longitud por máquina/control.
- Representación del portaherramientas y pruebas de colisión en la primera versión CAM.
- Modelo de selección de máquina (por proyecto, setup u operación).
- Algoritmo de contacto y estructura espacial inicial para STL de grandes dimensiones; tolerancias nominales y validación independiente.
- Primera operación CAM, dependencias completas de las trayectorias, política de reutilización/exportación y pruebas independientes. La huella por sí sola no basta.

## Desarrollo reciente de las decisiones D-024 a D-032

Estas decisiones constan en la documentación del repositorio revisada en `f1e436b`. No implican capacidad CAM ejecutable. La D-024 cambió **expresamente** el acuerdo anterior D-010: la biblioteca es la única fuente editable de geometría. La operación referencia un UUID, no una copia de fresa. La eventual trayectoria se calculará sobre una definición temporal e inmutable y necesitará comprobación de sus dependencias.

Las D-025 a D-028 hacen independiente del formato la elección de pivote visual y resuelven el gesto de pulsación/arrastre sin saltos; su precisión es la de la representación GPU, no la de contactos CAM. Las D-029 y D-030 establecen paneles movibles con estado independiente del proyecto. La D-031 mueve todas las preferencias propias a `config/` junto al ejecutable. La D-032 elimina el menú «Posición» en favor de guías y asa gráfica. El commit `599ca50` contiene el ajuste visual 018; el commit `f1e436b` añade UUID.

## D-033 — Compatibilidad durante fase experimental

- **Fecha:** 2026-11-14. **Aceptada expresamente por el responsable.**
- **Regla:** no invertir esfuerzo ahora en mantener compatibilidad ni migraciones de archivos `.lagucam` o ajustes anteriores. El paso v1 → v2 es admisible en desarrollo.
- **Condiciones:** documentar incompatibilidades y **no borrar/reemplazar silenciosamente archivos personales**. Reconsiderar la política antes de una versión estable.

## D-034 — Definición de huella geométrica SHA-256

- **Fecha:** 2026-11-14. **Aceptada como definición técnica.** Pendiente en `f1e436b`; **implementada desde `8e35c0e`**, sin integración con trayectorias.
- **Identidad frente a forma:** UUID sigue fijo al editar la fresa; revisión aumenta; SHA-256 identifica **la definición geométrica efectiva**, no el historial.
- **Incluir:** tipo de herramienta, todos los diámetros/radios/ángulos relevantes, longitud de corte, diámetro del mango y longitud total. Al añadir cuello, portaherramientas u otras cotas que intervengan en contacto/colisión, ampliar expresamente el esquema del hash.
- **Excluir:** UUID, revisión, nombre, notas, referencia comercial, RPM y avances recomendados. Los parámetros de mecanizado se tratarán separadamente para decidir si el G-code necesita regeneración.
- **Cálculo implementado:** serialización binaria **canónica y versionada** con orden/codificación inequívocos, unidades fijas y rechazo de valores geométricos inválidos. Normalizaciones exactas (`+0.0`/`-0.0`, etc.) deben definirse en la implementación y probarse; no usar el JSON de visualización.
- **Caso Ø6 → Ø4 → Ø6:** igual UUID, revisiones distintas, posibilidad de igual hash final al original **si coinciden todos los parámetros del hash**.
- **Validez futura de trayectorias:** capturar temporalmente la definición de la biblioteca durante el cálculo y registrar UUID, huella y demás dependencias. Una igualdad del hash de fresa **no autoriza por sí sola** reutilizar trayectoria o G-code si cambiaron modelo, bruto, setup, estrategia, algoritmo u otros factores.
- **Alcance realizado:** función independiente de hash y pruebas en `src/tools/hash.rs`. Las operaciones y `.lagucam` **no incorporan una huella persistida**. Su integración y la comprobación de resultados quedan para una etapa posterior.

## D-035 — Editor único de operaciones (prototipo)

- **Fecha de incorporación:** 2026-11-15, integración `7de83c9` / `13a439a`.
- Crear operación vacía en el árbol; abrir editor único con doble clic y pestañas General / Estrategia / Área; borrar desde el editor.
- La interfaz gestiona una previsualización experimental y no constituye una validación de trayectoria CNC.

## D-036 — Proceso de desarrollo y documentación

- **Fecha:** 2026-11-15. **Aceptación expresa del responsable en conversación de dirección.**
- Mantener el orden **debate, aceptación, implementación, validación y documentación**. Desarrollo y el chat de motor **no editan Markdown**; entregan resultados verificables para la última fase.
- La documentación puede retrasarse respecto a `main`; `docs/STATUS.md` debe indicar el **SHA cubierto** y la evidencia disponible. El HEAD de GitHub identifica el código existente, no su grado de validación.
- Documentar al **cerrar hitos**, no necesariamente cada commit. Evitar duplicar estados en README, arquitectura, requisitos y changelog.
- **Motivo:** evitar documentación prematura y contradicciones, sin bloquear el trabajo ni confundir lo implementado con lo comprobado.
- **Efecto:** sustituye la antigua exigencia de documentar dentro del mismo cambio de código, manteniendo D-017 (documentación viva) y D-016 (roles separados).

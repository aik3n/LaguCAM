# Registro de decisiones técnicas

**Última revisión:** 2026-11-14 · **Base:** commit `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`

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
| D-010 | **SUSTITUIDA** | Una operación conservaba una definición/copia de herramienta | Sustituida explícitamente por D-024 a petición del responsable del proyecto, 2026-11-14 |
| D-011 | **ACEPTADA** | Bibliotecas independientes de herramientas, máquinas y postprocesadores | El motor CAM no depende de FluidNC ni del modo de cambio de herramienta |
| D-012 | **ACEPTADA** | Primer postprocesador: FluidNC | Permite validar una máquina real, sin bloquear futuros controles |
| D-013 | **ACEPTADA** | Modo de cambio manual/automático pertenece a capacidades de máquina/control | La máquina actual cambia fresas a mano; otras pueden tener ATC |
| D-014 | **ACEPTADA** | No fijar T1/T2 a la geometría de la fresa | Asignación en configuración de máquina/trabajo |
| D-015 | **ACEPTADA** | Proyecto separado del STL externo; archivo con esquema versionado | Evitar duplicar gigabytes y permitir evolución del formato |
| D-016 | **ACEPTADA** | Dirección técnica y desarrollo en chats separados; GitHub como fuente del código | Debate/aprobación antes de implementar; pruebas y commits verificables |
| D-017 | **ACEPTADA** | Documentación viva en el repositorio, revisada por commit significativo | Poder reanudar otro chat con evidencia, sin depender de su memoria |
| D-018 | **ACEPTADA** | Para cambios de setup, no asumir que G54 efectúa rotación | G54 selecciona offsets; rotaciones requieren soporte/configuración explícitos del control |
| D-019 | **ACEPTADA** | No asumir que M6 ejecuta cambio manual en FluidNC | La secuencia exacta debe validarse con la configuración de la máquina |
| D-020 | **PROPUESTA** | Cinco tipos iniciales: plana, esférica, V, cónica plana y cónica esférica | Referencia DeskProto; por cerrar tras definir geometrías y fórmulas |
| D-021 | **PROPUESTA** | Primera estrategia: acabado paralelo con fresa esférica | Caso geométrico controlable antes de acometer desbaste y stock remanente |
| D-022 | **PROPUESTA** | Un perfil de máquina por proyecto, configurable | Simplificación inicial; el usuario no ha confirmado el modelo definitivo |
| D-023 | **PROPUESTA** | Archivo `.lagucam` en JSON versionado | Formato JSON versionado ya implementado en HEAD; ratificación formal de la decisión aún pendiente |
| D-024 | **ACEPTADA** | La herramienta pertenece exclusivamente a la biblioteca global; cada operación futura guarda solo su UUID, sin geometría ni parámetros ocultos | Cambios en biblioteca afectan al resolver la operación; falta de ID = error visible y bloqueo del cálculo; cualquier trayectoria anterior queda obsoleta y requiere nuevo cálculo. Aprobado por el responsable el 2026-11-14; no implementa aún operaciones |

## Cómo registrar una nueva decisión

Cada nueva entrada debe contener ID, fecha, estado (**PROPUESTA**, **ACEPTADA**, **SUSTITUIDA**, **DESCARTADA**), contexto del problema, alternativas consideradas, criterio elegido, impacto, pruebas necesarias y SHA si produjo código.

No convertir automáticamente una recomendación de un asistente en **ACEPTADA** sin respuesta del usuario. Si se cambia una decisión, enlazar la entrada a la decisión anterior y explicar las migraciones/repercusiones.

## Preguntas abiertas

- Definiciones y parámetros exactos de cada tipo de fresa; ángulo del cono total frente a semiángulo, radio de punta y límites geométricos.
- ¿Incluir toroidal y «ball with flat tip» en la primera entrega o después? Son perfiles distintos.
- Política de colocación de herramienta y offsets de longitud por máquina/control.
- Representación del portaherramientas y pruebas de colisión en la primera versión CAM.
- Modelo de selección de máquina (por proyecto, setup u operación).
- Algoritmo de contacto y estructura espacial inicial para STL de grandes dimensiones; tolerancias nominales y validación independiente.
- Alcance de las primeras operaciones, archivo y esquema persistentes, y gestión de trayectorias obsoletas.

## Nota de implementación de herramientas (sin cambiar decisiones previas)

La petición específica de implementar cinco geometrías autoriza el alcance de este cambio. Se documentan aquí las fórmulas en `LIBRARIES.md`, pero **D-020 conserva su estado de PROPUESTA** hasta la aceptación expresa del catálogo definitivo por el responsable. No se ratifican otras propuestas del registro ni se añaden decisiones sobre máquinas o postprocesado.

## Decisión D-025 — Navegación y formatos

- **Fecha:** 2026-11-14. **Estado:** ACEPTADA expresamente por el responsable en la conversación de desarrollo.
- **Contexto:** necesidad de fijar el pivote de órbita mediante doble clic, hoy sobre STL, sin condicionar STEP, IGES ni ningún formato futuro.
- **Criterio:** cámara/navegación independiente de importadores. La escena proporciona una consulta genérica de superficie visible; los importadores y renderizadores resuelven su propia representación. No cambiar el origen CNC ni los datos originales.
- **Alternativas:** recorrer el STL desde CPU (descartado por coste de STL grandes); construir un índice geométrico de todos los formatos ahora (fuera de alcance); lectura de profundidad renderizada (seleccionada para interacción visual).
- **Consecuencias:** la precisión del hit es la precisión visual/rasterizada y no es válida para CAM; MSAA requiere resolver profundidad. Si falla la lectura, no se mueve el pivote.
- **Pruebas requeridas:** matemáticas de proyección/desproyección, fondo sin superficie, MSAA, cambios de pivote, centrado y malla grande en la GPU real.
- **SHA de base:** `46f66fce07236e296a0c92dc27645b3b2e3e6cf8`; sin SHA de nuevo commit.

## D-026 — Clic simple sin desplazamiento de camara

- **Estado:** ACEPTADA por el responsable el 2026-11-14. Modifica el gesto de D-025 (doble clic -> clic simple); mantiene su principio de independencia de formatos.
- **Regla:** elegir una superficie cambia solo el pivote; ni punto de vista ni encuadre cambian durante el clic. Girar posteriormente usa el pivote elegido, con indicador situado sobre la proyeccion real del punto.
- **Sin superficie:** conservar el pivote. **Centrar vista:** volver al pivote central. **Origen CNC:** sus puntos de seleccion tienen prioridad, no deben interferir con el pivote visual.
- **Pruebas:** proyeccion invariada al elegir, punto fijo en pantalla durante arrastre, ventanas redimensionadas, MSAA y mallas grandes. Sin cambios de proyecto, herramientas ni G-code.

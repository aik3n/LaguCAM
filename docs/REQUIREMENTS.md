# Requisitos de LaguCAM

> **Requisitos y criterios**, no estado vivo de implementación. Estado actualizado y SHA cubierto: [`STATUS.md`](STATUS.md).


**Estado:** borrador vivo · **Revisión documental:** 2026-11-15. Los estados en tablas son hitos observados hasta `13a439a`; la evidencia detallada está en STATUS.

La columna de estado diferencia *validado*, *observado en código*, *acordado*, *propuesto* y *pendiente*. Los criterios técnicos aquí descritos sirven para convertir decisiones en tareas verificables.

## 1. Interfaz y proyecto

| ID | Requisito | Estado |
| --- | --- | --- |
| UI-01 | Visor tridimensional con rotación, zoom, desplazamiento y centrado | Usuario validó interacción |
| UI-02 | Abrir STL binarios cercanos a 1 GB sin perder fluidez al navegar en el equipo de referencia | Usuario validó caso real; faltan métricas |
| UI-03 | Árbol de proyecto y propiedades, biblioteca global desde menú principal | Implementado en código; selección de herramienta por operación pendiente |
| UI-04 | Bruto rectangular editable, tamaño, posición, extras por cada cara y representación visual | Observado en código; usuario probó bruto |
| UI-05 | Origen CNC con referencia a bruto o modelo, marcadores seleccionables, offset y orientación | Observado en código |
| PRJ-01 | Crear, abrir, guardar y guardar como proyecto `.lagucam` | Implementado en código desde 6eb12e1; validación gráfica de versión actual pendiente |
| PRJ-02 | Guardar referencia externa al modelo, bruto, origen, aspecto, operaciones, unidades y máquina seleccionada cuando exista | Acordado para persistencia; algunos campos aún no existen |
| PRJ-03 | Esquema versionado, validación, STL ausente/modificado y preservación del proyecto anterior ante fallos | `.lagucam` v2 implementado en código; validación gráfica pendiente |
| PRJ-04 | Declarar explícitamente unidades; STL no las incorpora | Acordado; milímetros como punto de partida, no deducir de STL |
| PRJ-05 | Trabajar sin copias gigantes de triángulos dentro del archivo de proyecto | Acordado |

## 2. Geometría y estrategias

| ID | Requisito | Estado |
| --- | --- | --- |
| GEO-01 | Conservar el modelo fuente sin modificaciones por visualización | Observado en código para STL |
| GEO-02 | Distinguir geometría real de renderización normalizada | Observado en código; requisito permanente |
| GEO-03 | Admitir STL primero y otros formatos, especialmente sólidos STEP, a futuro | Acordado; STEP pendiente |
| GEO-04 | Abstracciones por **capacidad geométrica**, sin obligar a convertir todo B-Rep a una malla única | Acordado como criterio arquitectónico |
| GEO-05 | Consultas eficientes de contacto/intersección para millones de triángulos | Prototipo de contacto esférico/BVH implementado; rendimiento real por validar |
| GEO-06 | Permitir representaciones auxiliares (p. ej. heightfield), con error definido | Acordado; implementación pendiente |
| CAM-01 | Estrategias iniciales estrictamente 3 ejes, husillo fijo paralelo al Z del setup | Acordado |
| CAM-02 | Posibilitar 4/5 ejes sin implementar hoy su cinemática | Acordado |
| CAM-03 | Desbaste, semiacabado, acabado, contorno, conforme se desarrollen | Previsto, no implementado |
| CAM-04 | Estrategias disponibles según capacidades del modelo, no solo extensión del fichero | Acordado |
| CAM-05 | Primera estrategia de prueba: acabado paralelo con fresa esférica | Prototipo TEST aceptado; muestreo/previsualización, sin trayectoria CNC válida |
| CAM-06 | Trayectorias y geometría con tolerancias de cálculo, error y validación | Acordado; implementación pendiente |

## 3. Herramientas, máquinas y control

| ID | Requisito | Estado |
| --- | --- | --- |
| TL-01 | Biblioteca **global**, reutilizable y sin carpetas obligatorias | Implementado en código |
| TL-02 | Filtros por nombre, tipo y diámetro; favoritas/recentes como mejora futura | Filtros implementados en código |
| TL-03 | Tipos inicialmente considerados: plana, esférica, cónica V, cónica punta plana y cónica punta esférica | **Propuesta de catálogo**, aún por cerrar |
| TL-04 | Editor con vista técnica de perfil y parámetros coherentes para cada tipo | Implementado en código; validación visual actual pendiente |
| TL-05 | Separar geometría cortante, mango/cuello, montaje y velocidades/avances | Acordado |
| TL-06 | Operación guarda UUID global sin snapshot; trayectoria futura compara huella y dependencias antes de usarse | UUID compartido implementado; resolución e invalidación pendientes |
| MACH-01 | Biblioteca de perfiles de máquina, independiente de herramientas y postprocesadores | Acordado |
| MACH-02 | Características físicas, límites, husillo y modo de cambio manual/ATC según máquina | Acordado; aún no implementado |
| MACH-03 | Perfil de la máquina de pruebas con FluidNC y cambio manual | Confirmado como caso de uso; valores físicos de captura por verificar |
| POST-01 | Postprocesadores independientes, primer objetivo FluidNC | Acordado; pendiente |
| POST-02 | Estrategias no generan directamente dialecto G-code de una máquina | Acordado |
| POST-03 | Validar secuencia de cambio manual sin asumir que `M6` hace el trabajo | Acordado; pendiente |
| POST-04 | No asumir que `G54` rota el espacio; transformar solo con soporte explícito del control | Acordado |

## 4. Requisitos no funcionales y de seguridad

| ID | Requisito | Criterio inicial |
| --- | --- | --- |
| NFR-01 | Rendimiento | Mantener la posibilidad de cargar/navegar un STL real cercano a 1 GB; medir antes de optimizar |
| NFR-02 | Gestión de memoria | Streaming, límites acotados en colas, errores recuperables; medir RAM/VRAM en GPU objetivo |
| NFR-03 | Precisión | `f64` para modelo y cálculos geométricos de proyecto; `f32` exclusivamente donde el pipeline visual lo justifica |
| NFR-04 | Trazabilidad | Guardar versión de formato, máquina, herramienta y parámetros que determinan cada trayectoria |
| NFR-05 | Validación | Unitarias, integración, geometrías patrón, regresión y verificación gráfica en máquina de pruebas |
| NFR-06 | Seguridad CNC | Revisión independiente de movimientos, clearance, límites, colisiones y post antes de usar G-code real |
| NFR-07 | Licencias | Proyecto MIT, verificar directas y transitivas por plataforma al integrar o distribuir |
| NFR-08 | Documentación | Sincronización **después de validación**, por hito; `STATUS.md` declara SHA cubierto; no atribuir pruebas no realizadas (D-036) |

## 5. No requisitos actuales

- No exigir STEP ni B-Rep para comenzar el primer cálculo con STL.
- No diseñar dinámicas de 5 ejes ni cinemática de máquina rotativa en la fase inicial.
- No desarrollar cambio automático solo porque exista un campo «número de herramientas» en DeskProto.
- No producir G-code operativo con el mero hecho de visualizar una trayectoria.
- No integrar el lector/renderizador de SolidCraft como requisito del proyecto.

## 6. Decisiones abiertas que afectan a requisitos

- Cinco geometrías ya implementadas con convenciones de ángulo total y radio de esfera; falta validación para cálculo CAM exacto.
- Grado de representación de mango, cuello y portaherramientas en la primera biblioteca.
- Semántica de máquina por proyecto o por setup (una máquina por proyecto fue **recomendada, no confirmada**).
- Tolerancias nominales y validación independiente del motor de contacto esférico ya implementado experimentalmente.
- Evolución del esquema `.lagucam` v2; en desarrollo no se exige compatibilidad con versiones anteriores.
- Unidad de longitud y conversión para formatos futuros.

## Requisitos añadidos tras D-033 y D-034

- **TL-10 — Huella geométrica (ACEPTADA, función implementada; integración CAM pendiente):** SHA-256 canónico y versionado sobre tipo, diámetros, radios, ángulos, longitud de corte, diámetro del mango, longitud total y otras cotas físicas pertinentes; sin UUID, revisión, nombre, notas, RPM o avance. Prueba Ø6 → Ø4 → Ø6 debe recuperar la huella cuando toda la geometría coincide.
- **TL-11 — Vigencia de resultados (ACORDADO COMO OBJETIVO; aún sin trazabilidad de trayectorias):** futuro cálculo resuelve UUID actual y toma copia temporal inmutable; trayectoria identifica hash geométrico y demás dependencias; prohibir exportación si están obsoletas.
- **PRJ-06 — Compatibilidad experimental (ACEPTADO):** durante desarrollo no mantener versiones anteriores ni migraciones obligatorias; comunicar incompatibilidades y no destruir archivos personales.
- **CAM-07 — Separación de validez:** cambios tecnológicos como avance y RPM pueden requerir regenerar G-code sin modificar la geometría calculada; una coincidencia de hash geométrico no valida por sí sola máquina, setup ni programa.

## CAM experimental: criterios abiertos

- El contacto de la punta esférica deberá contrastarse con referencias analíticas independientes, incluidos casos degenerados; una prueba de compilación no certifica geometría.
- Para una trayectoria CNC habrá que fijar tolerancia geométrica, error **entre** muestras, movimientos de entrada/retirada y prevención de colisiones de herramienta completa, bruto y utillaje.
- El índice espacial del STL grande requiere medición de tiempo de construcción, consultas y memoria real en modelos crecientes; la navegación fluida del visor no demuestra ese rendimiento.
- Las operaciones deberán resolver UUID de biblioteca, capturar herramienta durante el cálculo y verificar la huella antes de considerar un resultado vigente; el hash no autoriza por sí solo a exportar G-code.

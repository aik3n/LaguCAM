# Requisitos de LaguCAM

> **Requisitos y criterios**, no estado vivo de implementación. Estado actualizado y SHA cubierto: [`STATUS.md`](STATUS.md).


**Estado:** requisitos vigentes · **Revisión:** 2026-11-17. Los estados son observaciones estáticas del código público hasta `a10e0c0`; la evidencia de pruebas y la distinción local/remoto están en STATUS.

La columna de estado diferencia *validado*, *observado en código*, *acordado*, *propuesto* y *pendiente*. Los criterios técnicos aquí descritos sirven para convertir decisiones en tareas verificables.

## 1. Interfaz y proyecto

| ID | Requisito | Estado |
| --- | --- | --- |
| UI-01 | Visor tridimensional con rotación, zoom, desplazamiento y centrado | Usuario validó interacción |
| UI-02 | Abrir STL binarios cercanos a 1 GB sin perder fluidez al navegar en el equipo de referencia | Usuario validó caso real; faltan métricas |
| UI-03 | Árbol de proyecto y propiedades, biblioteca global desde menú principal | Implementado, incluido selector de fresa por operación; validación funcional pendiente |
| UI-04 | Bruto rectangular editable, tamaño, posición, extras por cada cara y representación visual | Observado en código; usuario probó bruto |
| UI-05 | Origen CNC con referencia a bruto o modelo, marcadores seleccionables, offset y orientación | Observado en código |
| PRJ-01 | Crear, abrir, guardar y guardar como proyecto `.lagucam` | Implementado en código desde 6eb12e1; validación gráfica de versión actual pendiente |
| PRJ-02 | Guardar referencia externa al modelo, bruto, origen, aspecto, operaciones, unidades y máquina seleccionada cuando exista | Acordado para persistencia; algunos campos aún no existen |
| PRJ-03 | Esquema versionado, validación, STL ausente/modificado y preservación del proyecto anterior ante fallos | `.lagucam` v2 implementado en código; validación gráfica pendiente |
| PRJ-04 | Declarar explícitamente unidades; STL no las incorpora | Selección de unidad origen mm/pulgadas implementada en `ModelTransform`, con conversión física a mm; validación integral pendiente |
| PRJ-05 | Trabajar sin copias gigantes de triángulos dentro del archivo de proyecto | Acordado |
| PRJ-07 | Modelo STL no destructivo; escala, rotación, traslación, pivote y AABB físico persistentes | P1-M2 implementado en código; prueba Windows de visor, BVH y round-trip pendiente |

## 2. Geometría y estrategias

| ID | Requisito | Estado |
| --- | --- | --- |
| GEO-01 | Conservar el modelo fuente sin modificaciones por visualización | Observado en código para STL |
| GEO-02 | Distinguir geometría real de renderización normalizada | Observado en código; requisito permanente |
| GEO-03 | Admitir STL primero y otros formatos, especialmente sólidos STEP, a futuro | Acordado; STEP pendiente |
| GEO-04 | Abstracciones por **capacidad geométrica**, sin obligar a convertir todo B-Rep a una malla única | Acordado como criterio arquitectónico |
| GEO-05 | Consultas eficientes de contacto/intersección para millones de triángulos | ContactSolver esférico, plano y toroidal/BVH implementado; rendimiento real por validar |
| GEO-06 | Permitir representaciones auxiliares (p. ej. heightfield), con error definido | Acordado; implementación pendiente |
| CAM-01 | Estrategias iniciales estrictamente 3 ejes, husillo fijo paralelo al Z del setup | Acordado |
| CAM-02 | Posibilitar 4/5 ejes sin implementar hoy su cinemática | Acordado |
| CAM-03 | Desbaste, semiacabado, acabado, contorno, conforme se desarrollen | Previsto, no implementado |
| CAM-04 | Estrategias disponibles según capacidades del modelo, no solo extensión del fichero | Acordado |
| CAM-05 | Primera estrategia de prueba: acabado paralelo con fresa esférica | TEST con raster, movimiento neutral y enlaces propuestos; sin trayectoria CNC segura |
| CAM-06 | Trayectorias y geometría con tolerancias de cálculo, error y validación | Validador estructural G25 implementado; tolerancias entre muestras y colisiones pendientes |
| CAM-08 | Trayectoria neutral separada de estrategia y post | G25 implementado; enlaces propuestos y validación estructural, sin exportación CNC |
| CAM-09 | Contrato F/S y conversión al origen de trabajo 3 ejes | F1.1–F1.2 implementados; comprobación de máquina y unidades pendiente |
| CAM-10 | Invalidación de resultados cuando cambie cualquier entrada geométrica significativa | P1-A `CamProvenance` implementado para previews, con firma STL muestreada; contrato exportable pendiente |

## 3. Herramientas, máquinas y control

| ID | Requisito | Estado |
| --- | --- | --- |
| TL-01 | Biblioteca **global**, reutilizable y sin carpetas obligatorias | Implementado en código |
| TL-02 | Filtros por nombre, tipo y diámetro; favoritas/recentes como mejora futura | Filtros implementados en código |
| TL-03 | Familias de fresas: plana, esférica, toroidal, cónica V, cónica punta plana y cónica punta esférica | Seis variantes en biblioteca; solver solo para plana/esférica/toroidal |
| TL-04 | Editor con vista técnica de perfil y parámetros coherentes para cada tipo | Implementado en código; validación visual actual pendiente |
| TL-05 | Separar geometría cortante, mango/cuello, montaje y velocidades/avances | Acordado |
| TL-06 | Operación guarda UUID global sin snapshot; trayectoria futura compara huella y dependencias antes de usarse | UUID, resolución y SHA de herramienta implementados para previews; dependencias globales pendientes |
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

- Seis geometrías de biblioteca implementadas; contacto CAM para esférica/plana/toroidal, sin validación independiente completa ni soporte de las tres cónicas.
- Grado de representación de mango, cuello y portaherramientas en la primera biblioteca.
- Semántica de máquina por proyecto o por setup (una máquina por proyecto fue **recomendada, no confirmada**).
- Tolerancias nominales y validación independiente del motor de contacto esférico ya implementado experimentalmente.
- Evolución del esquema `.lagucam` v2; en desarrollo no se exige compatibilidad con versiones anteriores.
- Unidad de longitud y conversión para formatos futuros.

## Requisitos añadidos tras D-033 y D-034

- **TL-10 — Huella geométrica (ACEPTADA, integrada parcialmente en previsualizaciones):** SHA-256 canónico y versionado sobre tipo, diámetros, radios, ángulos, longitud de corte, diámetro del mango, longitud total y otras cotas físicas pertinentes; sin UUID, revisión, nombre, notas, RPM o avance. Prueba Ø6 → Ø4 → Ø6 debe recuperar la huella cuando toda la geometría coincide.
- **TL-11 — Vigencia de resultados (PARCIAL):** el cálculo ya resuelve UUID y registra firma geométrica para previsualización; faltan modelo, bruto, setup, estrategia, tolerancias y post antes de reutilizar/exportar. No autorizar exportación de resultados obsoletos.
- **PRJ-06 — Compatibilidad experimental (ACEPTADO):** durante desarrollo no mantener versiones anteriores ni migraciones obligatorias; comunicar incompatibilidades y no destruir archivos personales.
- **CAM-07 — Separación de validez:** cambios tecnológicos como avance y RPM pueden requerir regenerar G-code sin modificar la geometría calculada; una coincidencia de hash geométrico no valida por sí sola máquina, setup ni programa.

## CAM experimental: criterios abiertos

- El contacto de la punta esférica deberá contrastarse con referencias analíticas independientes, incluidos casos degenerados; una prueba de compilación no certifica geometría.
- Para una trayectoria CNC habrá que fijar tolerancia geométrica, error **entre** muestras, movimientos de entrada/retirada y prevención de colisiones de herramienta completa, bruto y utillaje.
- El índice espacial del STL grande requiere medición de tiempo de construcción, consultas y memoria real en modelos crecientes; la navegación fluida del visor no demuestra ese rendimiento.
- Las operaciones resuelven UUID y capturan identidad geométrica para previews; `CamProvenance` añade STL/bruto/transformación/parámetros. La comprobación de vigencia exportable y de colisiones sigue pendiente; el hash de herramienta no autoriza por sí solo G-code.

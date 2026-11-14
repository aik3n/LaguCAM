# Requisitos de LaguCAM

**Estado:** borrador vivo, no todo implementado · **Base:** `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`

La columna de estado diferencia *validado*, *observado en código*, *acordado*, *propuesto* y *pendiente*. Los criterios técnicos aquí descritos sirven para convertir decisiones en tareas verificables.

## 1. Interfaz y proyecto

| ID | Requisito | Estado |
| --- | --- | --- |
| UI-01 | Visor tridimensional con rotación, zoom, desplazamiento y centrado | Usuario validó interacción |
| UI-02 | Abrir STL binarios cercanos a 1 GB sin perder fluidez al navegar en el equipo de referencia | Usuario validó caso real; faltan métricas |
| UI-03 | Panel lateral con modelo, bruto, origen, herramientas y operaciones ordenadas | Observado en código; herramientas aún vacías |
| UI-04 | Bruto rectangular editable, tamaño, posición, extras por cada cara y representación visual | Observado en código; usuario probó bruto |
| UI-05 | Origen CNC con referencia a bruto o modelo, marcadores seleccionables, offset y orientación | Observado en código |
| PRJ-01 | Crear, abrir, guardar y guardar como proyecto `.lagucam` | **Pendiente**; etapa 1 acordada |
| PRJ-02 | Guardar referencia externa al modelo, bruto, origen, aspecto, operaciones, unidades y máquina seleccionada cuando exista | Acordado para persistencia; algunos campos aún no existen |
| PRJ-03 | Versionar esquema, validar al cargar, tratar STL ausente/modificado y no perder el proyecto anterior por error | Acordado como criterio de la etapa 1 |
| PRJ-04 | Declarar explícitamente unidades; STL no las incorpora | Acordado; milímetros como punto de partida, no deducir de STL |
| PRJ-05 | Trabajar sin copias gigantes de triángulos dentro del archivo de proyecto | Acordado |

## 2. Geometría y estrategias

| ID | Requisito | Estado |
| --- | --- | --- |
| GEO-01 | Conservar el modelo fuente sin modificaciones por visualización | Observado en código para STL |
| GEO-02 | Distinguir geometría real de renderización normalizada | Observado en código; requisito permanente |
| GEO-03 | Admitir STL primero y otros formatos, especialmente sólidos STEP, a futuro | Acordado; STEP pendiente |
| GEO-04 | Abstracciones por **capacidad geométrica**, sin obligar a convertir todo B-Rep a una malla única | Acordado como criterio arquitectónico |
| GEO-05 | Consultas eficientes de contacto/intersección para millones de triángulos | Pendiente |
| GEO-06 | Permitir representaciones auxiliares (p. ej. heightfield), con error definido | Acordado; implementación pendiente |
| CAM-01 | Estrategias iniciales estrictamente 3 ejes, husillo fijo paralelo al Z del setup | Acordado |
| CAM-02 | Posibilitar 4/5 ejes sin implementar hoy su cinemática | Acordado |
| CAM-03 | Desbaste, semiacabado, acabado, contorno, conforme se desarrollen | Previsto, no implementado |
| CAM-04 | Estrategias disponibles según capacidades del modelo, no solo extensión del fichero | Acordado |
| CAM-05 | Primera estrategia sugerida: acabado paralelo con fresa esférica | **Propuesta**, no aprobada todavía como desarrollo |
| CAM-06 | Trayectorias y geometría con tolerancias de cálculo, error y validación | Acordado; implementación pendiente |

## 3. Herramientas, máquinas y control

| ID | Requisito | Estado |
| --- | --- | --- |
| TL-01 | Biblioteca **global**, reutilizable y no organizada por carpetas obligatorias | Acordado |
| TL-02 | Búsqueda y filtros por nombre, tipo y diámetro; favoritas/recentes como mejora posible | Acordado el enfoque; filtros futuros |
| TL-03 | Tipos inicialmente considerados: plana, esférica, cónica V, cónica punta plana y cónica punta esférica | **Propuesta de catálogo**, aún por cerrar |
| TL-04 | Editor con vista técnica de perfil y parámetros coherentes para cada tipo | Acordado como concepto |
| TL-05 | Separar geometría cortante, mango/cuello, montaje y velocidades/avances | Acordado |
| TL-06 | Las operaciones retienen una definición identificable; cambios de biblioteca no alteran silenciosamente cálculos previos | Acordado |
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
| NFR-08 | Documentación | Actualización asociada a cada cambio significativo, sin atribuir pruebas no realizadas |

## 5. No requisitos actuales

- No exigir STEP ni B-Rep para comenzar el primer cálculo con STL.
- No diseñar dinámicas de 5 ejes ni cinemática de máquina rotativa en la fase inicial.
- No desarrollar cambio automático solo porque exista un campo «número de herramientas» en DeskProto.
- No producir G-code operativo con el mero hecho de visualizar una trayectoria.
- No integrar el lector/renderizador de SolidCraft como requisito del proyecto.

## 6. Decisiones abiertas que afectan a requisitos

- Conjunto exacto de cinco geometrías iniciales y convenciones dimensionales/angulares.
- Grado de representación de mango, cuello y portaherramientas en la primera biblioteca.
- Semántica de máquina por proyecto o por setup (una máquina por proyecto fue **recomendada, no confirmada**).
- Estrategia de cálculo geométrico inicial y tolerancias nominales.
- Formato definitivo de proyecto (`.lagucam` y JSON propuestos en el encargo de etapa 1; no existe aún esquema ratificado en código).
- Unidad de longitud y conversión para formatos futuros.

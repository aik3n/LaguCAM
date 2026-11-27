# LaguCAM — contratos vigentes CAM / NC

Este documento especifica **responsabilidades**, no certifica su cumplimiento funcional ni la seguridad de una máquina. Flujo: **estrategia -> planificador -> postprocesador -> NC definitivo -> intérprete -> verificador y simulador**. La configuración de máquina aporta datos; la UI informa; el operador prepara la CNC.

## C1. Estrategia CAM

- Produce trayectoria **neutral** con corte, aproximación, retirada y enlaces internos entre pasadas.
- Empieza en su **primer XY**, a la **Z de seguridad de esa operación**; no obliga a desplazarse primero al origen o a un aparcamiento.
- No emite G-code, G53, M6, homing ni transiciones entre operaciones. No conoce la posición física inicial de la CNC.

## C2. Planificador de operaciones

- Recibe operaciones **activas y ordenadas**, trayectorias válidas, fresas y condiciones de mecanizado.
- Produce enlaces neutrales entre operaciones y solicitudes de herramienta **T**, incluso repetidas, cuando correspondan. Una operación individual es un caso del mismo motor interno de planificación.
- Para enlazar operaciones, usa desplazamientos **G0 en Z de seguridad**, primero XY a altura suficiente y después el ajuste de Z de entrada, respetando los límites geométricos realmente conocidos; no asume que el espacio está libre si no ha sido verificado.
- No transforma los cortes ni inventa condiciones de despeje. La salida individual y la conjunta mantienen sus contratos específicos.

## C3. Postprocesador

- Traduce el plan neutral a **G-code definitivo** del controlador configurado (cabecera, modos, F/S, T/M6, G53 y final cuando procedan).
- **No calcula trayectorias ni enlaces**. Emitir M6 no significa ejecutar ni verificar un cambio físico de herramienta.
- Mantiene reglas diferenciadas para NC individual y conjunto, aunque compartan el motor interno de emisión.

## C4. Configuración de máquina

- Describe recorridos, límites, capacidades, posiciones de retirada y procedimientos configurados; proporciona datos a planificación/post.
- Los datos descriptivos no son límites comprobados físicamente. Una configuración FluidNC/GRBL no demuestra comportamiento seguro de la CNC.
- Máquina y post son configuraciones portables independientes de `.lagucam` en el flujo actual.

## C5. Intérprete NC

- Analiza el **texto NC definitivo** y reconstruye movimientos y estados; comunica instrucciones no soportadas e inconsistencias.
- No modifica G-code ni decide cómo mecanizar; no presupone que un comando desconocido es inocuo.

## C6. Simulador

- Recibe los movimientos interpretados y calcula **material restante orientativo** con un Heightfield de 3 ejes verticales.
- Ofrece **resultado final** y **reproducción progresiva**, usando el mismo motor, sin alterar G-code ni trayectoria.
- No representa socavados ni colisiones completas de herramienta, mango, portaherramientas, fijaciones o máquina. No es una herramienta de certificación CNC.

## C7. Verificador NC

- Evalúa exclusivamente comprobaciones **implementadas**, devuelve errores, advertencias e información referenciados a movimientos u operaciones cuando sea posible.
- No reescribe silenciosamente el NC ni garantiza ausencia de colisiones o seguridad física sin comprobaciones suficientes.

## C8. Interfaz UI/UX

- Presenta incidencias **en contexto**, con severidad clara y sin duplicaciones innecesarias.
- Un **error real** bloquea solo la acción afectada; las advertencias informan sin requerir aceptación genérica de riesgos.
- No exige casillas de reconocimiento para generar/exportar NC; las casillas **funcionales** (activar/ocultar operación) sí son válidas.
- La **visibilidad** de una operación es independiente de su **activación** y de la inclusión en NC.

## C9. Operador

- Es responsable del homing, origen de trabajo real, posición inicial física, herramienta montada, fijaciones, límites y preparación del controlador.
- LaguCAM no sustituye dichas comprobaciones ni convierte el G-code generado en físicamente seguro.

## Invariantes transversales

- **CAM neutral != NC final != simulación != verificación de seguridad**. No intercambiar las responsabilidades.
- El STL original y la geometría CAM de precisión no se sustituyen por la malla de visualización.
- Las validaciones y pruebas deben asociarse a una versión y escenario concretos; una inspección estática no equivale a ejecución.
- Los cambios de contratos requieren aprobación explícita del usuario, inspección de los módulos afectados y pruebas de regresión.

Contexto de producto y prioridad actual: [`CONTEXTO.md`](CONTEXTO.md).

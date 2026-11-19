# Estado verificable de LaguCAM

<!-- LAGUCAM-MASTER-DOC-CUT: 85dfda28658d5f8e779caae97838742d1444be73 -->

**Corte documental:** 2026-11-18. **Codigo publico inspeccionado (solo lectura):** [`85dfda28658d5f8e779caae97838742d1444be73`](https://github.com/aik3n/LaguCAM/commit/85dfda28658d5f8e779caae97838742d1444be73) (`main`, «operaciones multiples»). **No se ha inspeccionado la carpeta local ni ejecutado cargo, GUI o CNC en esta auditoria.** Un SHA publicado no acredita pruebas del mismo SHA ni que el Escritorio coincida con GitHub.

## Evidencia y limites del corte

| Evidencia | Alcance |
| --- | --- |
| Base del codigo | `85dfda2`, inspeccion estatica de GitHub; comparacion con `0f4c811` |
| Pruebas del corte | Tests unitarios presentes en codigo, **ejecucion sobre este SHA no acreditada aqui** |
| Evidencias historicas | 130 tests Windows informados en `7de83c9` y 161/161 declarados en `d12ebcc`; **no transferir** esos resultados a `85dfda2` |
| STL grande | Navegacion de STL cercano a 1 GB comunicada por el usuario para un estado anterior; sin benchmark CAM/BVH en este corte |
| Seguridad CNC | NC **experimental**; no hay validacion integral de colisiones, fijaciones, portaherramientas, limites de maquina ni certificacion FluidNC |
| Proximo commit documental | No existe hasta que el usuario aplique, valide y publique este parche; los chats no realizan Git |

## Capacidades implementadas inspeccionadas hasta `85dfda2`

| Area | Estado en codigo | Limite pendiente |
| --- | --- | --- |
| Proyecto y visor | STL externo, visor OpenGL, bruto, cero CNC, `.lagucam` v2, unidades STL y transformaciones no destructivas | Pruebas integrales de transformaciones/visor/BVH; rendimiento medido |
| Fresas y geometria | Biblioteca UUID, SHA-256 geometrico y ContactSolver para plana, esferica y toroidal | Validacion geometrica independiente y colisiones de herramienta completa |
| Acabado paralelo TEST | Contactos, sobreespesor, zigzag, movimientos neutrales, aproximacion y despeje | No es desbaste por capas ni trayectoria certificada |
| Procedencia CAM | `CamProvenance`, invalidacion de resultados por entradas significativas, firma STL economica | No equivale a hash completo del STL ni verificacion fisica |
| Operacion individual | Acciones Calcular / Generar NC / Simular; calcula CAM si falta resultado vigente | Revisar interacciones con calculos concurrentes y fallos de integracion |
| Operaciones multiples | `cam_batch.rs` encola solo operaciones activas en orden; reutiliza validas, calcula de una en una y permite cancelar | **No** hay post conjunto, union de trayectorias ni simulacion conjunta |
| Reordenar operaciones | Reasignacion de indices de calculos, cache, seleccion y cola; flags `active` persistidos | Regresion durante trabajo activo y edicion concurrente |
| Post | `nc_post.rs` genera G-code 3 ejes, unidades, modal, Start/End configurables, preview y exportacion experimental | No es perfil fisico FluidNC verificado; Start/End pueden alterar el programa |
| Interpretacion NC | `nc_interpreter.rs` interpreta texto final G0/G1 y modos soportados, rechaza instrucciones desconocidas | **Rechaza G53 y G54**, asi como arcos y varios M; no cubre preparacion de maquina |
| Informes NC | Metricas del NC interpretado (G0/G1 y tiempo G1 sin aceleraciones), alertas preliminares seleccionables en visor | Start personalizado invalida metricas automaticas si su origen previo no se conoce; no es tiempo real |
| Reproduccion NC | Traza interpretada; sesion ligada a origen STL, transformacion, bruto, NC y fresa; modo compacto | Posicion inicial compacta es **teorica CAM**, no posicion fisica confirmada |
| Material 2,5D | Heightfield plano/esferico, historial de cambios con limites, rebobinado, OpenGL de material/fresa | No simula socavados, stock real previo, portaherramientas ni fijaciones |
| Verificacion preliminar | Envolvente del bruto original, posibles cruces G0, husillo y otros avisos | No sustituye colisiones, material remanente ni prueba de maquina |

## Diferencia introducida por `85dfda2` respecto a `0f4c811`

- Nuevo `src/cam_batch.rs`: cola de calculo por operaciones activas; las no activas se excluyen o se saltan si se desactivan durante la ejecucion.
- `main.rs`: coordinacion secuencial de un solo trabajo CAM, reutilizacion de resultados, errores por operacion, cancelacion y reasignacion de indices durante reordenacion.
- `project_ui.rs`: accion **Calcular** en el nodo conjunto; **Generar NC** y **Simular** conjuntos continuan deshabilitados.
- `test_parallel.rs`: reutilizacion del calculo existente y cancelacion de la tarea activa sin eliminar resultados validos de otras operaciones.

## Contrato de inicio de estrategia y maquina (decisiones D-045 a D-047)

**Aceptado por el responsable:** cada estrategia comienza en **el XY inicial de la operacion**, con **Z en la altura de seguridad definida para esa estrategia por encima del bruto** (o cota superior geometrica equivalente calculada). No genera movimientos XY ajenos a la operacion para buscar aparcamiento o seguridad de maquina. Los enlaces XY **internos** entre pasadas siguen permitidos a la cota de despeje definida, sujetos a verificacion.

**Responsabilidad del operador:** posicion inicial fisica, homing, fijacion, herramienta y origen de trabajo. **Responsabilidad del perfil/post futuro:** si se configura, retirada vertical en coordenadas de maquina (p. ej. `G53 G0 Z...`) y llegada al XY de inicio; requiere homing, valor MPos seguro, despeje comprobado y movimientos trazables. `G53` **no esta implementado** en el interprete NC del corte y Start personalizado puede bloquear interpretacion/simulacion automatica. **No anunciarlo como funcional.**

## Riesgos y trabajo abierto

1. **Seguridad mecanica:** entradas profundas de los archivos NC experimentales de ejemplo, ausencia de planificacion de desbaste por niveles y verificacion de barrido, fijaciones y maquina; no realizar corte real con estos ejemplos sin revision.
2. **Contrato NC maquina:** G53 opcional/configurable, semantica de estados iniciales, transiciones entre operaciones y distincion entre origen CNC y coordenadas maquina; requerira perfil de maquina y ampliacion del interprete/verificador.
3. **Pruebas de lote:** activar/desactivar/reordenar durante calculo, cancelar, error intermedio, reutilizar una operacion, borrar operacion y cambiar proyecto, sin resultados asociados a indices incorrectos.
4. **UI y rendimiento:** validar fisicamente el modo compacto/aislado y FPS/memoria del renderizado de material a distintas resoluciones.
5. **Mantenimiento:** `test_parallel.rs` y `nc_export_ui.rs` concentran responsabilidades; refactorizar solo por defecto o necesidad de contrato demostrado (D-044). Revisar fallback silencioso `preview_matrix()` a identidad visual.
6. **Nomenclatura historica:** G19/G25/P2-D2 en nombres, comentarios y tests; limpieza localizada pendiente sin perder cobertura.

**Proximo objetivo:** validar el codigo `85dfda2` en Windows, documentar pruebas reales; consolidar lote y flujo individual; definir contrato de limites del post/maquina, **sin dar por cerrados P1–P4**. Consultar [`ROADMAP.md`](ROADMAP.md), [`ARCHITECTURE.md`](ARCHITECTURE.md), [`DECISIONS.md`](DECISIONS.md) y [`TESTING.md`](TESTING.md).

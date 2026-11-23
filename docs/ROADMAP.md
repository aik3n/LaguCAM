# Hoja de ruta de LaguCAM — prototipo CAM de tres ejes

<!-- LAGUCAM-MASTER-DOC-CUT: 6d496200f5bf6ec70bb0bb8249543b315d3e1854 -->

**Prioridades revisadas:** 2026-11-23 · **Base inspeccionada:** `6d496200` (solo codigo publico). Estado y evidencia en [`STATUS.md`](STATUS.md). Este documento no autoriza por si mismo nuevos desarrollos.

## Objetivo del primer cierre

Flujo reproducible **STL → proyecto, bruto y origen → herramienta UUID → acabado paralelo → trayectoria neutral → post NC → interpretacion y simulacion del NC definitivo → diagnosticos → exportacion experimental controlada**. El uso fisico requiere pruebas y validaciones adicionales de maquina; no inferir seguridad a partir de avisos vacios.

## Estado actual

| Area | Avance inspeccionado | Sin cerrar |
| --- | --- | --- |
| P1 Contratos y procedencia | Herramienta UUID/hash, STL fisico, origen de trabajo, invalidacion CAM y NC | Regresion integral en Windows, divergencia visual posible en fallo de matriz, trazabilidad completa por escenario |
| P2 Movimiento, simulacion, verificacion | Acabado paralelo TEST, enlaces, interprete NC, simulador heightfield y avisos de envolvente | Barrido entre muestras, colisiones reales y portaherramientas, errores geometricos y tiempos fisicos |
| P3 Post y maquina | Post generico 3 ejes y exportacion NC experimental | Perfil FluidNC validado, limites/homing, retirada G53 configurable, transiciones y NC machine-safe |
| P4 UX e integracion | Operaciones directas, simulacion compacta, informe NC, resalte de avisos | Pruebas UI de regresion, claridad de estados y rendimiento |
| Cálculo y NC conjunto | Cola de operaciones activas, `cam_plan.rs` para enlaces neutrales y `cam_combined_nc.rs` para programa conjunto | Sin planificación de obstáculos, simulación de material multiherramienta ni validación física CNC |
| Posterior | STEP, 4/5 ejes, desbaste, estrategias adicionales | Fuera del primer cierre |

## Proximas tareas ordenadas

### 1. Completar validación de `6d496200` sin mezclar cambios

- `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo test --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release` y prueba de GUI Windows, con salidas asociadas a un SHA.
- Pruebas end-to-end de operacion individual y lote: cache reutilizada, error de una operacion, cancelacion, activar/desactivar, reordenar durante calculo, abrir otro proyecto, caducidad de resultado y progreso. Medir memoria cuando proceda.
- Reproducir proyectos con transformaciones del STL, origen G54, cambio de herramienta, resumen NC, alertas y simulacion compacta. Un informe sin incidencias no certifica seguridad CNC.

### 2. Fijar y probar el contrato de estrategia (D-045)

- Entrada neutral exactamente en `(XY_inicio_operacion, Z_seguridad_operacion)`; Z referida a la cima/seguridad del bruto segun la geometria de referencia que defina la estrategia.
- Ningun salto XY a aparcamiento/home al empezar; los desplazamientos internos entre pasadas siguen dentro del dominio de la estrategia y su despeje.
- Validar coordenadas y continuidad hasta el ultimo movimiento neutral; separar posicionamiento externo de los enlaces internos. No permitir que el post cambie silenciosamente la geometria de corte.

### 3. Diseñar el contrato de preparacion de maquina, antes de implementarlo (D-046)

- Identificar la posicion fisica y homing como responsabilidad del operador; el post prepara el acceso al primer XY de CAM sin imputarlo a la estrategia.
- Perfil de maquina opcional: retirada **solo Z** en coordenadas maquina con `G53 G0 Z...`, cota configurable y validada; despues transito XY a despeje confirmado y llegada a Z segura de operacion.
- `G53` aun no es interpretable en LaguCAM; ampliar estados, frames y verificador con pruebas de referencias de maquina o mantener modo de salida no verificable. No admitir silenciosamente comandos de Start que alteren el recorrido.

### 4. Validar y extender el planificador neutral ya implementado (D-048)

- `cam_plan.rs` ya enlaza operaciones con movimientos subir–XY–bajar a la mayor Z pertinente. `cam_combined_nc.rs` genera un único NC para las operaciones activas vigentes, sujeto a sus restricciones; **no** es concatenación sin plan.
- Pendientes: colisiones de herramienta completa, volumen barrido, fijaciones, límites de máquina, cambios físicos T/M6 y estados externos de control.
- Probar operaciones con cotas y herramientas distintas, cancelación, vigencia, orden, visibilidad oculta y exportación. No confundir geometría neutral con recorrido seguro en una CNC.
- UX pendiente: normalización progresiva de iconos Lucide y redistribución/tamaño de las ventanas del editor tras la limpieza de textos y controles redundantes.

### 5. Verificacion de mecanizado y rendimiento

- Material remanente, volumen barrido, manga/mango/portaherramientas, fijaciones, entradas verticales profundas y alturas reales; distinguir advertencia conservadora de colision demostrada.
- Benchmarks de STL grande, BVH, simulacion 2,5D, CPU/GPU/memoria y pruebas sin material en maquina cuando exista alcance aprobado.

## Responsables y barreras

**Usuario:** decide, ejecuta y verifica; solo el hace Git. **Chat Master:** audita y documenta despues de validacion. **Integrador CAM:** estado coherente y flujo extremo a extremo. **Acabado y Motor:** contrato de trayectoria/colisiones. **Post/Maquina:** G53, perfil, estados, transiciones. **UI/UX:** presentacion sin alterar contratos. Desbaste, STEP y 5 ejes pospuestos.

D-036–D-044 regulan parches locales: un solo `.ps1` cada vez, con `-CheckOnly`, preflight y sin crear backups automaticos. Un SHA remoto no sustituye el estado local. **No se declara cerrado ningun hito P1–P4 sin evidencia del propio hito.**

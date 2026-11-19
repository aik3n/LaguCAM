# LaguCAM — Contratos de responsabilidad

**Estado:** ACEPTADO por el responsable del proyecto el 2026-11-19.  
**Referencia de inspección:** `dd228bfac29b8cf6e0930dc5cfdf2e5831f6ddbd` (GitHub público).  
**Alcance:** especificación de arquitectura y UX; **no implica implementación ni validación funcional**.

Este documento es la referencia para distribuir responsabilidades. Las decisiones D-045 y D-046 siguen vigentes; D-048 formaliza el reparto completo y acepta el principio planteado en D-047.

## Flujo de responsabilidad

Operación y estrategia → planificador → postprocesador → NC definitivo → intérprete → verificador y simulador.

La configuración de máquina proporciona datos y capacidades. La interfaz presenta resultados e incidencias. El operador prepara y maneja físicamente la CNC.

## C1. Estrategia CAM

- **Produce:** trayectoria neutral con corte, aproximaciones, retiradas y enlaces internos entre pasadas.
- **Inicio:** primer XY de mecanizado, a la Z de seguridad definida por la estrategia sobre el bruto.
- **No hace:** posicionamiento general de máquina, homing, aparcamiento, G53, M6 ni generación de G-code.
- **Límite:** no conoce la posición inicial física de la CNC ni decide transiciones entre operaciones.

## C2. Planificador de operaciones

- **Recibe:** operaciones activas en su orden, trayectorias vigentes, herramientas y condiciones de mecanizado.
- **Produce:** plan neutral conjunto con movimientos de transición y solicitudes de herramienta T, incluso si se repiten.
- **No hace:** generar G-code ni modificar los movimientos de corte de las estrategias.
- **Regla:** una operación individual es un caso de planificación con una sola operación.
- **Límite:** no presupone efectos físicos de M6 ni inventa movimientos cuando faltan condiciones para planificarlos.

## C3. Postprocesador

- **Recibe:** plan neutral y configuración de controlador/máquina aplicable.
- **Produce:** G-code definitivo, con cabecera, final, modos y sintaxis del controlador.
- **Gestiona:** comandos específicos configurados, como G53 y solicitudes T/M6.
- **No hace:** calcular cortes ni enlaces geométricos entre operaciones.
- **Límite:** emitir M6 no equivale a verificar que el controlador efectúe un cambio físico correcto.

## C4. Configuración de máquina

- **Define:** recorridos, límites, posición de retirada G53 y capacidades físicas/de control disponibles.
- **Proporciona:** datos al planificador y al postprocesador.
- **No hace:** calcular trayectorias ni generar G-code.
- **Límite:** un dato configurado no debe presentarse como comprobado si no existe verificación implementada.

## C5. Intérprete NC

- **Recibe:** el texto G-code definitivo.
- **Produce:** movimientos y estados interpretados para los consumidores posteriores.
- **Informa:** instrucciones no soportadas o inconsistencias de interpretación.
- **No hace:** modificar G-code ni decidir cómo mecanizar.
- **Límite:** una instrucción no soportada no se presume inocua ni ejecutada correctamente.

## C6. Simulador

- **Recibe:** los movimientos y estados reconstruidos por el intérprete NC.
- **Produce:** representación del mecanizado y cálculo de eliminación de material según sus capacidades.
- **Mantiene:** estado propio del bruto y de las herramientas.
- **No hace:** modificar trayectorias ni G-code.
- **Límite:** no atribuye a una simulación parcial comprobaciones físicas que no implementa.

## C7. Verificador NC

- **Recibe:** los movimientos y estados del intérprete y los datos pertinentes.
- **Produce:** incidencias concretas dentro de las verificaciones realmente implementadas.
- **Relaciona:** cada incidencia con su movimiento, operación o componente cuando sea posible.
- **No hace:** corregir silenciosamente trayectorias ni G-code.
- **Límite:** no declara ausencia de colisiones o seguridad física sin comprobaciones suficientes.

## C8. Interfaz UI/UX

- **Presenta:** estados e incidencias en contexto, preferentemente junto a la operación o componente afectado.
- **Severidades:** error, advertencia e información, identificables mediante iconos y detalle accesible.
- **Interrupción:** solo cuando un error técnico impide la acción concreta solicitada.
- **No hace:** duplicar avisos, exigir aceptación genérica de riesgos ni utilizar casillas de verificación para confirmar advertencias.
- **Aclaración:** esta prohibición no afecta a casillas funcionales normales (por ejemplo, activar una operación).

## C9. Operador

- **Responsable:** homing, posición física inicial, origen de trabajo, herramientas, fijaciones y preparación/configuración real del controlador.
- **LaguCAM no exige:** casillas de reconocimiento de estas responsabilidades para generar o exportar G-code.
- **Límite:** esta distribución no convierte el NC en físicamente seguro ni sustituye las comprobaciones necesarias antes del mecanizado.

## Regla transversal de incidencias

| Icono | Severidad | Comportamiento |
| --- | --- | --- |
| Error | Problema técnico que impide una acción concreta | Bloquear únicamente la acción afectada y explicar la causa |
| Advertencia | Posible problema que merece atención | Mostrar en contexto, sin confirmaciones obligatorias |
| Información | Dato relevante | Mostrar de forma discreta y consultable |

Una incidencia debe tener un mensaje coherente y un lugar claro de consulta; no repetirse por distintas pantallas.

## Alcance del acuerdo

Estos son **contratos aceptados**, no una declaración de que el código actual los cumple. Su implementación se abordará por cambios pequeños, con pruebas y aceptación del usuario conforme a `WORKFLOW.md`. No se modifica código ni se autoriza maquinaria mediante este documento.

# Protocolo de trabajo: usuario, chat de dirección y chat de desarrollo

**Estado:** protocolo inicial propuesto para adopción · **Fecha:** 2026-11-14

## 1. Roles

**Responsable del proyecto (usuario):** fija objetivos, preferencias, acepta o descarta decisiones y prueba la interfaz/mecanizado en su equipo. Sus observaciones reales tienen prioridad frente a estimaciones de un asistente, con registro del alcance observado.

**Chat de dirección técnica:** debate, elabora decisiones, evalúa impactos, revisa entregas y prepara una instrucción de desarrollo **solo si se le pide**. No debe producir trabajo accesorio ni ampliar la salida innecesariamente.

**Chat de desarrollo (Work u otro entorno con código):** analiza el repositorio real, ejecuta el cambio acordado, prueba, documenta y entrega. No reinterpreta libremente objetivos ni reclama validación que no puede realizar.

**GitHub:** referencia autoritativa del estado del código versionado. Los archivos Markdown dentro del repositorio documentan decisiones y evidencia; la conversación, por sí sola, no es sustituto del historial.

## 2. Ciclo de trabajo

1. **Debate.** La dirección técnica y el usuario deciden qué problema resolver. Se registra si es una decisión cerrada o propuesta.
2. **Especificación breve.** Solo cuando el usuario la solicita: objetivo, alcance excluido, criterios de aceptación y restricciones.
3. **Verificación inicial.** Desarrollo lee `AGENTS.md`, `docs/STATUS.md`, `docs/DECISIONS.md` y el código del SHA actual; revisa cambios locales antes de tocar nada.
4. **Implementación acotada.** Cambios graduales; sin refactorizaciones oportunistas, nuevas dependencias injustificadas ni modificaciones no solicitadas.
5. **Pruebas.** Las disponibles en el entorno, más tests relevantes del cambio. Registrar comandos, sistema operativo y errores.
6. **Revisión de regresión.** No degradar visor de STL grande, bruto, origen ni proyectos. La prueba gráfica del usuario puede ser necesaria.
7. **Documentación.** Actualizar las secciones afectadas **en el mismo conjunto de cambios**.
8. **Entrega.** Proporcionar SHA/patch, resumen breve, resultados, limitaciones y qué debe verificar el usuario.
9. **Validación.** El usuario prueba la funcionalidad. La dirección decide si acepta, pide correcciones o abre otra etapa.

## 3. Regla de actualización por commit

Para todo commit con impacto funcional o arquitectónico:

- `docs/CHANGELOG.md`: añadir entrada con SHA, qué cambió y cómo se validó.
- `docs/STATUS.md`: actualizar capacidades/evidencia y SHA de referencia. **No marcar USUARIO-VALIDADO antes de recibir esa validación.**
- `docs/ROADMAP.md`: mover hitos solo tras aceptación; registrar bloqueos reales.
- `docs/REQUIREMENTS.md`: actualizar estado de requisitos afectados.
- `docs/ARCHITECTURE.md`: actualizar cuando cambien flujos, invariantes o responsabilidades.
- `docs/LIBRARIES.md`: actualizar cuando cambie modelo de datos de herramienta, máquina o post.
- `docs/DECISIONS.md`: registrar decisiones nuevas o sustituidas aprobadas; no inventar aprobaciones.
- `AGENTS.md` / `docs/WORKFLOW.md`: modificar **solo si cambia de forma deliberada el proceso de colaboración**.

Un commit que **solo actualiza documentación** no debe atribuirse pruebas de código que no se hicieron. Al actualizar `STATUS.md` dentro del mismo commit, registrar claramente el SHA base del código revisado y sustituirlo por el SHA de la entrega una vez conocido; si el agente no puede confirmar el SHA final, dejarlo como «por confirmar», nunca inventarlo.

## 4. Plantilla de informe de entrega

```text
Objetivo: <una frase>
Base: <SHA real>
Entrega: <SHA real, o 'sin commit'>
Cambios: <archivos/funciones principales>
Pruebas: <comando + resultado, o 'no ejecutado'>
Prueba en equipo del usuario: <pendiente / resultado comunicado>
Riesgos: <máximo 3 relevantes>
Documentación: <archivos actualizados>
Siguiente decisión: <solo si hay una decisión pendiente>
```

## 5. Reglas de colaboración con chat

- **No generar un prompt cuando el usuario quiere debatir.** Primero resolver la duda y, si se solicita, redactar el encargo.
- Respuestas precisas, sin felicitaciones excesivas, relleno, planes largos ni propuestas automáticas de funciones.
- Si la pregunta es concreta, contestarla primero. No lanzar nuevas preguntas cuando no bloquean la decisión.
- Ante un SHA, inspeccionar el commit indicado antes de evaluarlo; no dar por bueno que `main` coincide sin comprobarlo.
- No afirmar que se puede compilar, ejecutar Work, cambiar GitHub o confirmar seguridad si no existe acceso real y resultado constatado.
- No seguir la premisa errónea de que G54 implica rotación de ejes o de que FluidNC gestiona M6 de forma universal.
- No perder de vista que **calcular una trayectoria y ejecutar G-code real son dos niveles de validación diferentes**.
- Mantener separados la consulta técnica, la decisión, la especificación y la implementación.

## 6. Cómo retomar LaguCAM desde otro chat

Enviar la URL del repositorio y el SHA a revisar, con `AGENTS.md` y el índice como referencia. Instrucción inicial corta sugerida:

> «Trabajaremos sobre LaguCAM. Lee `AGENTS.md` y `docs/INDEX.md`, revisa `docs/STATUS.md` y `docs/DECISIONS.md` y comprueba el HEAD de GitHub. No modifiques código todavía. Dime en pocas líneas el estado probado, lo pendiente y cualquier discrepancia importante entre documentación y código». 

Después el responsable indicará el rol (debate o desarrollo) y una tarea concreta. **El siguiente chat no debe inventar continuidad si el repositorio ha cambiado.**

## 7. Política de Git

- Identificar siempre SHA base y estado de cambios no confirmados.
- Usar commits pequeños y mensajes descriptivos; dejar fácil revertir una regresión.
- No sobrescribir archivos o modificaciones no confirmadas sin consentimiento.
- No publicar, fusionar ni modificar la rama principal por iniciativa propia.
- Los STL reales grandes quedan fuera de Git; usar un STL pequeño de referencia y datos sintéticos para tests reproducibles.
- Una tarea puede terminar correctamente con un patch o archivo sin commit **si no se dispone de acceso Git**, pero declararlo explícitamente.

## 8. Aprobación y control de alcance

Una tarea termina cuando los criterios acordados se cumplen o cuando se informa de un bloqueo reproducible. Un agente no debe autoasignarse una nueva etapa. Si propone una mejora, presentarla como **propuesta futura**, no implementarla ni preguntar automáticamente al usuario si quiere autorizarla.

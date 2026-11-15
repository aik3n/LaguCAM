# Flujo de trabajo y sincronización documental de LaguCAM

**Protocolo vigente desde:** 2026-11-15. Autoridad: aprobación del responsable del proyecto.

## 1. Cinco fases, en este orden

1. **Debate.** Usuario y dirección técnica delimitan el problema, las alternativas y los riesgos. Una idea no es una decisión aprobada.
2. **Aceptación.** El usuario autoriza alcance y criterios de salida. El chat de dirección prepara una instrucción breve **solo si se solicita**.
3. **Implementación.** El chat de programación, o el especializado en motor, parte de HEAD verificado; modifica código y pruebas sin ampliar el alcance. **No edita documentación**.
4. **Validación.** Pruebas Rust y geométricas, revisión del commit y comprobación del usuario si es necesaria. La entrega especifica evidencias, plataforma, resultados y limitaciones. Es legítimo detenerse por una validación insuficiente.
5. **Documentación.** Este chat actualiza, tras la validación, `docs/STATUS.md` y los documentos afectados; separa código observado de pruebas acreditadas y documenta decisiones realmente aceptadas.

La documentación se puede agrupar **por hitos**, no por cada commit. No bloquear la implementación por la sincronización documental pendiente; sí detener cualquier afirmación de «validado» que carezca de pruebas.

## 2. Dos referencias, nunca una supuesta sincronización

`docs/STATUS.md` debe mostrar:

- **HEAD de código observado al hacer la revisión**, con rama y SHA.
- **Código cubierto por la documentación:** SHA concreto hasta el que el documento ha sido reconciliado con el repositorio. Este SHA **no** se actualiza por la mera existencia de commits posteriores.
- **Evidencia disponible:** pruebas ejecutadas en qué sistema, por quién y contra qué versión; resultados pendientes o no reproducidos.

Al iniciar cualquier nueva tarea: comparar HEAD con el SHA documentado. Si son diferentes, **examinar los commits nuevos antes de usar las afirmaciones de STATUS**. Las decisiones estables siguen vigentes, salvo modificación explícita.

## 3. Un solo documento para cada función

| Documento | Responsabilidad |
| --- | --- |
| `docs/STATUS.md` | Único estado vigente: implementado, probado y pendiente, con SHAs |
| `docs/DECISIONS.md` | Registro de decisiones aprobadas, propuestas o sustituidas; no estado de compilación |
| `docs/ARCHITECTURE.md` | Componentes, contratos, invariantes y límites; evitar repetir el estado actual |
| `docs/REQUIREMENTS.md` | Requisitos y criterios de aceptación; remitir a STATUS para su grado de cumplimiento |
| `docs/ROADMAP.md` | Objetivos, dependencias e hitos; no inventariar cada commit |
| `docs/TESTING.md` | Cómo comprobar y qué evidencia se exige; las ejecuciones concretas se registran en STATUS |
| `docs/LIBRARIES.md` | Definiciones de herramientas, máquinas y posts |
| `docs/CHANGELOG.md` | Historia fechada de entregas y decisiones, sin actuar como otro STATUS |
| `README.md` | Presentación, compilación y enlaces al estado vigente |
| `AGENTS.md` / `docs/WORKFLOW.md` | Reglas estables de colaboración; no fijar un SHA «actual» permanente |

No crear un documento nuevo para cada entrega salvo que contenga información técnica que no encaje en los anteriores. Conservar archivos históricos existentes cuando tengan valor probatorio; corregir enlaces rotos.

## 4. Entrega mínima para pasar de validación a documentación

```text
Objetivo aprobado:
SHA base / SHA final:
Archivos y comportamiento modificados:
Pruebas: comando + sistema + resultado; evidencia o informe disponible:
Prueba manual del responsable: resultado concreto / pendiente:
Limitaciones técnicas o de seguridad:
Incompatibilidades de formato o datos:
Estado: aceptado / requiere corrección / validación pendiente:
```

El chat de documentación comprueba el commit y no adopta sin atribución frases como «130 tests superados» o «fluido en STL grande». Cuando no puede verificar una ejecución, la atribuye al informe de quien la realizó.

## 5. Política de Git y archivos personales

- Identificar siempre SHA base, rama y cambios locales antes de modificar.
- Commits de código pequeños y trazables; consolidación documental posterior con su propio commit.
- No publicar, fusionar ni reescribir `main` sin autorización del responsable.
- No eliminar STL personales ni sobrescribir configuraciones por conveniencia. Los STL grandes reales quedan fuera de Git salvo decisión expresa.
- Durante desarrollo no se requieren migraciones de formatos; informar de las incompatibilidades y no destruir archivos del usuario.
- Si falta permiso de escritura en GitHub, entregar archivos o parche exactos, **sin afirmar que se ha publicado un commit**.

## 6. Control de calidad y seguridad CAM

Compilar ≠ ejecutar correctamente ≠ calcular geometría precisa ≠ disponer de una trayectoria CNC segura. Evaluar según corresponda: pruebas de referencia independientes, tolerancias, rendimiento con mallas grandes, regresión del visor y flujo de verificación de la máquina. La interfaz puede visualizar muestras sin que estas se consideren trayectorias listas para postprocesar.

## 7. Cómo reanudar desde otro chat

Proporcionar repositorio, SHA o rama a revisar y rol del chat. Leer `AGENTS.md`, `docs/INDEX.md` y `docs/STATUS.md`; contrastar HEAD y las decisiones. Si la documentación va por detrás, indicarlo y consultar el código real, no inferir que el desarrollo está detenido.

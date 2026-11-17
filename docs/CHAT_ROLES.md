# Organización de chats, contratos y responsabilidades

**Decisión de gobierno:** 2026-11-17 · D-040–D-043. Este documento define **funciones**, no sesiones con identidad permanente. El responsable humano puede abrir, cerrar o reemplazar cualquier chat, incluido el Master.

## 1. Estructura operativa

```text
                    USUARIO (acepta, prueba, Git, máquina)
                                   |
                 MASTER (arquitectura, auditoría, documentación)
                                   |
                   INTEGRADOR CAM (flujo completo)
                   /           |           |          \
        GEOMETRÍA       ACABADO       MÁQUINA/POST     UI/UX
             (Desbaste: inactivo hasta cerrar el prototipo)
```

**Regla principal:** existe **un responsable de integración** que debe demostrar que una operación real atraviesa todos los contratos; no basta con que cada módulo se compile por separado. El Master conserva independencia para rechazar evidencias insuficientes o proponer correcciones, pero **no aprueba en lugar del usuario**.

## 2. Mandatos por rol

| Rol | Produce | No puede hacer sin autorización |
| --- | --- | --- |
| **Master** | Prioridades, decisiones propuestas, auditorías críticas, asignación, aceptación de evidencia, parche documental y `HANDOFF` | Commit, editar producción por comodidad, certificar pruebas que no ejecutó, convertir recomendaciones en autorización |
| **Integrador CAM** | P1–P4, contratos de operación y resultado, unidades, trazabilidad, consistencia de las interfaces, tests end-to-end; coordina merges **conceptuales** | Modificar contratos del motor/post sin consenso; aplicar parches por cuenta propia; Git |
| **Motor geométrico** | BVH, ContactSolver, casos analíticos, precisión, rendimiento, intersecciones y verificación geométrica | Editar UI, persistencia, máquina o post por decisión unilateral |
| **Estrategia de acabado** | Muestreo/acabado paralelo, control entre muestras, secuencia de corte, enlace/entrada/retirada en contrato neutral | Cambiar invariantes geométricos, máquinas o G-code dentro de estrategia |
| **Máquina y postprocesado** | Perfil de máquina, capacidades FluidNC, límites, transformación de trabajo, post aislado, tests golden y modos CNC | Suponer capacidades físicas, autorizar máquina, corregir trayectoria alterando la geometría calculada |
| **UI/UX** | Usabilidad, visor, editor, incidencias y exportación guiada; prueba manual con escenarios reales | Inventar estados «seguro/listo», alterar algoritmos o guardar datos nuevos sin contrato |
| **Desbaste** | *Futuro*: desbaste, bruto remanente y estrategias adicionales | Empezar antes de que el usuario apruebe una fase posterior al primer prototipo |

**El chat que desarrolla no certifica por sí solo su cambio:** el Integrador revisa integraciones y el Master audita límites, pruebas y arquitectura. Sin acceso a Windows, la ejecución final siempre la acredita el usuario.

## 3. Propiedad orientativa de módulos

| Área | Propietario técnico preferente | Regla de frontera |
| --- | --- | --- |
| `src/cam_geometry/*` | Motor geométrico | Interfaces de consulta acordadas con Integrador y Acabado |
| `src/toolpath.rs`, `src/toolpath_links.rs` | Acabado | Movimientos neutrales sin G-code ni control de máquina |
| `src/toolpath_validation.rs` y futura verificación continua | Integrador + Motor (responsable único por tarea) | Criterios de seguridad acordados; no confundir validez estructural con ausencia de colisiones |
| `src/test_parallel.rs` | Integrador (orquestación); Acabado (algoritmo) | **Archivo compartido:** toda modificación tiene un solo responsable y revisión cruzada; extraer lógica UI cuando P1/P2 lo requiera, no por longitud |
| `src/project/*`, `src/cam_contract.rs`, `src/tools/cam_adapter.rs` | Integrador | Identidad, unidades, transforms y persistencia con pruebas de invariantes |
| Futuro `src/post/*` y máquina | Máquina/Post | Post aislado de render/estrategias; sin exportación sin precondiciones verificadas |
| `src/main.rs`, `src/project_ui.rs`, `src/render.rs`, `src/*_ui.rs` | UI/UX, coordinado con Integrador | No alterar dominio CAM para facilitar dibujado |
| `AGENTS.md`, `docs/*.md`, `README.md` | Master | Solo parche documental autorizado y tras validación del hito, salvo actualización explícita de gobierno |

La tabla no asigna permisos permanentes para modificar archivos: **la lista exacta se aprueba por hito**. Cualquier cambio de contrato debe incluir consumidores afectados, estrategia de migración experimental si procede, tests y revisión de efectos secundarios.

## 4. Activación por etapa y concurrencia

- **Ahora (P1):** Master + Integrador. El Integrador completa la coherencia de fuentes, transformaciones, procedencia, F/S y contratos, sin invocar un grupo de especialistas para cada archivo.
- **P2:** activar Motor geométrico y Acabado con contratos de consulta, error y resultado predefinidos. Trabajan en investigación paralela y entregan parches **secuencialmente**.
- **P3:** activar Máquina/Post tras aprobar capacidades reales de FluidNC y las precondiciones de exportación.
- **P4:** activar UI/UX junto al Integrador para integrar diagnóstico, visualización, errores y exportación.
- **Después del primer flujo:** decidir si procede un especialista de desbaste. No abrir chats por simetría organizativa.

**Prohibición operativa:** dos parches en aplicación/validación a la vez o preparados suponiendo bases locales incompatibles. Antes de aceptar un segundo, resolver el primero (validado o corregido/retirado). Se permite debate y revisión sin modificar archivos. Si un parche altera módulos compartidos, anunciar el cambio de contrato al Master y a los consumidores antes de prepararlo.

## 5. Proceso de una tarea

1. **Master** propone hito, definición de terminado, responsables, riesgos e invariantes; usuario acepta.
2. **Responsable designado** inspecciona el estado local disponible y genera **un parche `.ps1`** con base verificada y plan de pruebas.
3. **Usuario** precomprueba, aplica, ejecuta pruebas Windows y verifica la interfaz o el resultado CAM; comparte salida y deficiencias.
4. **Integrador** confirma que los contratos consumidores siguen cumpliéndose; **Master** audita independientemente evidencia y alcance.
5. **Usuario** acepta o rechaza el cierre. Solo entonces **Master** prepara el parche documental correspondiente. El usuario realiza el commit cuando lo decida.

En revisiones, distinguir **defecto demostrado**, **riesgo fundado pendiente de reproducir**, **mejora opcional** y **deuda técnica no prioritaria**. Ordenar por impacto sobre corrección geométrica, seguridad, reproducibilidad, rendimiento medido y mantenibilidad; nunca por estética del código.

## 6. Sucesión del Master

El Master actual no es indispensable ni permanente. Al relevarlo, el usuario designa al siguiente y le entrega **`docs/HANDOFF.md`, `AGENTS.md`, `DECISIONS.md`, `STATUS.md`, `ROADMAP.md`, `WORKFLOW.md` y evidencia reciente**. La sucesión **no modifica los contratos vigentes ni concede autorización para implementar**. El nuevo Master contrasta el estado local con lo documentado y registra lagunas, sin inventar qué parches se aplicaron.

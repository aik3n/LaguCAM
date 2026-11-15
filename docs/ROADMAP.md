# Hoja de ruta de LaguCAM

**Revisión:** 2026-11-15. Este documento expresa prioridades y criterios de salida, **no** es otra copia del estado de código. Ver [`STATUS.md`](STATUS.md) para SHA y validaciones. No implica fechas ni autoriza automáticamente nuevas tareas.

## Etapas

| Hito | Resultado esperado | Situación |
| --- | --- | --- |
| **0.1 — Visor STL grande** | Abrir y mover en 3D STL reales cercanos a 1 GB | **USUARIO-VALIDADO** para el equipo y archivo probados; faltan métricas automatizadas |
| **0.2 — Entorno de proyecto** | Árbol lateral, bruto, origen, representación coherente | **IMPLEMENTADO** en código; comprobaciones funcionales positivas del usuario |
| **Etapa 1 — Persistencia** | Nuevo/abrir/guardar/guardar como, rutas STL externas, archivo versionado, unidades | **IMPLEMENTADO EN CÓDIGO** (v2); prueba manual actual pendiente |
| **Etapa 2 — Biblioteca de herramientas** | Editor de geometrías, perfil gráfico, catálogo global y selector filtrado | **EDITOR Y BIBLIOTECA IMPLEMENTADOS**; UUID global unificado; selector por operación pendiente |
| **Etapa 3 — Máquina y post** | Perfiles de máquina; abstracción post independiente; primer dialecto FluidNC | **ACORDADO COMO OBJETIVO**, sin implementación |
| **Etapa 4 — Base geométrica CAM** | STL, consultas espaciales, contacto herramienta-pieza y tests patrón | **PROTOTIPO IMPLEMENTADO; VALIDACIÓN ABIERTA** |
| **Etapa 5 — Primera estrategia** | Acabado paralelo 3 ejes con fresa esférica | **TEST DE CONTACTOS/PREVISUALIZACIÓN**, no trayectoria válida |
| **Etapa 6 — Verificación y exportación controlada** | Trayectorias visibles, comprobaciones de colisión/retirada/recorrido, post FluidNC, validación segura | **PENDIENTE** |
| **Etapas posteriores** | Desbaste, semiacabado, contorno, stock remanente, optimización, B-Rep/STEP, 4/5 ejes según demanda | **FUERA DEL ALCANCE INICIAL** |

No hay dependencia obligatoria de desarrollar la pantalla de máquinas antes de investigar las consultas geométricas. Se pueden estudiar subsistemas en paralelo, pero **un G-code utilizable requiere que herramientas, geometría, setup, máquina y post estén integrados y verificados**.

## Criterios de salida de cada etapa

### Persistencia

- Round-trip de parámetros, sin alteración de `f64`.
- Referencias relativas a STL, error recuperable ante falta/cambio de archivo.
- Guardado seguro ante fallos; versión de esquema y validación.
- Datos cargados no sobrescritos por los valores por defecto del importador.
- Prueba manual de abrir un proyecto real tras reiniciar; se conservará la fluidez del visor.

### Herramientas

- Tipos y fórmulas geométricas aprobados.
- Validación de dimensiones y perfiles de prueba con casos degenerados.
- Biblioteca global implementada; referencia UUID sin snapshot; selector filtrado desde operación aún pendiente.
- SHA-256 geométrico canónico v1 implementado como función; queda por conectar UUID y huella a resultados de cálculo y a otras dependencias.
- Visualización técnica y datos de corte no confundidos con geometría.

### Motor geométrico

- Consultas independientemente testeables sin la GUI.
- Resultados analíticos de contacto con planos, rampas, esferas y geometrías patrón.
- Tolerancia explícita y pruebas de errores máximos.
- Rendimiento comparado en mallas crecientes, incluyendo el STL de montaña cuando esté disponible.
- Herramientas planas/esféricas primero; ampliaciones solo tras geometría verificada.

### Post y seguridad

- Trayectoria neutral y G-code separados.
- Capacidad real de FluidNC y configuración de la máquina verificadas.
- Números de herramienta, cambio manual y offsets tratados sin supuestos inseguros.
- Simulación/comprobación offline, ejecución en vacío y pruebas controladas antes del mecanizado real.

## Próximo hito técnico sugerido (pendiente de autorización específica)

1. Validar el contacto esférico con referencias independientes, degeneraciones y tolerancias numéricas explícitas.
2. Medir el BVH y las consultas sobre modelos crecientes y el STL real de referencia; registrar RAM/RSS y tiempos, no solo límites estimados.
3. Controlar el error entre muestras del acabado paralelo; no generar G-code sobre puntos conectados sin verificar el recorrido.
4. Vincular las operaciones a la biblioteca UUID y, al existir resultados reutilizables, a hash geométrico y todas sus dependencias.
5. Después, cerrar trayectoria neutral y criterios de seguridad antes de abordar FluidNC. STEP y nuevas estrategias no preceden a la validación del motor inicial.

**Política de desarrollo:** se permiten cambios de esquema incompatibles sin migración; siempre documentados y sin borrar datos personales.

## Riesgos estratégicos

- Optimizar el render cuando ya cumple el objetivo sin medir un cuello de botella.
- Empezar algoritmos CAM sin definición formal de herramienta/tolerancia.
- Confundir los valores de una captura de DeskProto con los parámetros reales de una máquina FluidNC.
- Convertir cada idea del debate en una orden de programación y dispersar el esfuerzo.
- Confiar en la memoria de un chat sin mantener `STATUS.md`, decisiones y changelog en el repositorio.

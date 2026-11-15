# Hoja de ruta de LaguCAM

> **Avance local 15-11-2026:** la rama `integracion/cam-20261115` incorpora motor de contacto y acabado paralelo TEST. Sin G-code ni garantía CNC. La sección de «Priorización inmediata tras f1e436b» documenta un momento anterior.

**Revisión:** 2026-11-14. Es un plan secuencial, no una promesa de fechas. La entrada de nuevas funciones debe basarse en resultados y en decisiones explícitas del responsable.

## Etapas

| Hito | Resultado esperado | Situación |
| --- | --- | --- |
| **0.1 — Visor STL grande** | Abrir y mover en 3D STL reales cercanos a 1 GB | **USUARIO-VALIDADO** para el equipo y archivo probados; faltan métricas automatizadas |
| **0.2 — Entorno de proyecto** | Árbol lateral, bruto, origen, representación coherente | **IMPLEMENTADO** en código; comprobaciones funcionales positivas del usuario |
| **Etapa 1 — Persistencia** | Nuevo/abrir/guardar/guardar como, rutas STL externas, archivo versionado, unidades | **IMPLEMENTADO EN CÓDIGO** (v2); prueba manual actual pendiente |
| **Etapa 2 — Biblioteca de herramientas** | Editor de geometrías, perfil gráfico, catálogo global y selector filtrado | **EDITOR Y BIBLIOTECA IMPLEMENTADOS**; UUID global unificado; selector por operación pendiente |
| **Etapa 3 — Máquina y post** | Perfiles de máquina; abstracción post independiente; primer dialecto FluidNC | **ACORDADO COMO OBJETIVO**, sin implementación |
| **Etapa 4 — Base geométrica CAM** | Acceso geométrico fiable a STL, consultas espaciales, contacto herramienta-pieza y tests patrón | **PROTOTIPO IMPLEMENTADO Y PROBADO**; ampliación/validación pendiente |
| **Etapa 5 — Primera estrategia** | Acabado 3D paralelo de 3 ejes sobre superficies de referencia | **TEST FUNCIONAL**; no válido aún para mecanizado |
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
- SHA-256 geométrico canónico v1 implementado en el ajuste 020 (pendiente de ejecutar Cargo); futuras trayectorias compararán huella y otras dependencias para evitar exportar resultados obsoletos.
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

## Priorización inmediata tras `f1e436b`

1. **UUID unificado en código:** biblioteca, operaciones y `.lagucam` v2. Comprobaciones de compilación y GUI del commit actual pendientes de acreditar.
2. **Definición del SHA-256 geométrico:** aceptada. Ajuste 020 preparado: hash binario canónico/versionado y tests, incluido Ø6 → Ø4 → Ø6, sin ampliar operaciones ni persistencia; pendiente de validar en Windows.
3. Posteriormente debatir el registro de dependencias, validez de trayectorias y condiciones de exportación, antes del motor CAM.
4. Mantener documentación y pruebas de regresión; no invertir ahora en más cambios de interfaz ni en refactorizaciones cosméticas.

**Política de desarrollo:** se permiten cambios de esquema incompatibles sin migración; siempre documentados y sin borrar datos personales.

## Riesgos estratégicos

- Optimizar el render cuando ya cumple el objetivo sin medir un cuello de botella.
- Empezar algoritmos CAM sin definición formal de herramienta/tolerancia.
- Confundir los valores de una captura de DeskProto con los parámetros reales de una máquina FluidNC.
- Convertir cada idea del debate en una orden de programación y dispersar el esfuerzo.
- Confiar en la memoria de un chat sin mantener `STATUS.md`, decisiones y changelog en el repositorio.

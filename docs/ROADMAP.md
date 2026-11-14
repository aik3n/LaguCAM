# Hoja de ruta de LaguCAM

**Revisión:** 2026-11-14. Es un plan secuencial, no una promesa de fechas. La entrada de nuevas funciones debe basarse en resultados y en decisiones explícitas del responsable.

## Etapas

| Hito | Resultado esperado | Situación |
| --- | --- | --- |
| **0.1 — Visor STL grande** | Abrir y mover en 3D STL reales cercanos a 1 GB | **USUARIO-VALIDADO** para el equipo y archivo probados; faltan métricas automatizadas |
| **0.2 — Entorno de proyecto** | Árbol lateral, bruto, origen, representación coherente | **IMPLEMENTADO** en código; comprobaciones funcionales positivas del usuario |
| **Etapa 1 — Persistencia** | Nuevo/abrir/guardar/guardar como, rutas STL externas, archivo versionado, unidades | **PRÓXIMO; PENDIENTE** |
| **Etapa 2 — Biblioteca de herramientas** | Editor de geometrías, perfil gráfico, catálogo global y selector filtrado | **DISEÑO EN DISCUSIÓN** |
| **Etapa 3 — Máquina y post** | Perfiles de máquina; abstracción post independiente; primer dialecto FluidNC | **ACORDADO COMO OBJETIVO**, sin implementación |
| **Etapa 4 — Base geométrica CAM** | Acceso geométrico fiable a STL, consultas espaciales, contacto herramienta-pieza y tests patrón | **PENDIENTE** |
| **Etapa 5 — Primera estrategia** | Acabado 3D paralelo de 3 ejes sobre superficies de referencia | **PROPUESTA**, pendiente de aprobar estrategia exacta |
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
- Biblioteca global + selección filtrada + snapshot en proyecto.
- Al cambiar geometría, invalidación explícita de trayectorias dependientes.
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

## Priorización inmediata

1. Estabilizar documentación y su protocolo por commit.
2. Ejecutar y validar **persistencia** ya definida.
3. Debatir a fondo las **geometrías de fresa** antes de encargar su implementación.
4. No perder tiempo ahora con interfaz de SolidCraft, STEP completo, ATC, G-code avanzado o 5 ejes.

## Riesgos estratégicos

- Optimizar el render cuando ya cumple el objetivo sin medir un cuello de botella.
- Empezar algoritmos CAM sin definición formal de herramienta/tolerancia.
- Confundir los valores de una captura de DeskProto con los parámetros reales de una máquina FluidNC.
- Convertir cada idea del debate en una orden de programación y dispersar el esfuerzo.
- Confiar en la memoria de un chat sin mantener `STATUS.md`, decisiones y changelog en el repositorio.

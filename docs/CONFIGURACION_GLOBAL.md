# Configuración global de máquina y postprocesador (UX-01E.4)

## Decisión de arquitectura

- **Operación CAM:** estrategia, fresa de la biblioteca, condiciones de corte y número T opcional.
- **Planificador de NC:** ordena operaciones activas, enlaza trayectorias y transmite **cada** solicitud T, incluso si se repite. Sin T no solicita cambio.
- **Postprocesador global:** traduce las solicitudes a instrucciones compatibles con un controlador. En esta versión admite `Tn M6` o un modo sin M6; no hay deduplicación.
- **Máquina global:** mantiene datos físicos descriptivos independientes del post y del proyecto.
- **Controlador CNC:** ejecuta el cambio de herramienta y decide cómo gestionarlo. LaguCAM no deduce ni ejecuta ese procedimiento.

## Archivos portables

En el directorio `config/` junto al ejecutable:

- `machine.json`: una configuración global de máquina; versión de formato 1.
- `postprocessor.json`: un único postprocesador global, con el formato `PostProcessorProfile` existente.

Ambos pueden importarse y exportarse independientemente desde `Herramientas`.
Las configuraciones se validan antes de guardarse y se escriben de forma atómica.
La configuración de máquina y el postprocesador no se copian a `.lagucam`: el proyecto usa los vigentes al generar el NC.

## Compatibilidad

`config/postprocessors.json` (biblioteca anterior) **no se borra ni sobrescribe**. Deja de utilizarse como biblioteca activa. Si contiene perfiles personalizados, deben migrarse de forma explícita a `postprocessor.json`; la importación estándar acepta el JSON de **un perfil**, no la biblioteca completa.

La plantilla inicial del postprocesador es FluidNC con solicitud `Tn M6`; también se puede seleccionar una plantilla básica GRBL 1.1 sin M6. Las plantillas solo reemplazan el borrador hasta pulsar **Guardar**.

## Límites y seguridad

Los recorridos, velocidad máxima y RPM de máquina son **datos informativos** en esta fase. No certifican colisiones, límites reales, fixtures ni seguridad de cambios de herramienta. No se inventan límites físicos cuando faltan datos.

`M6` exige que el controlador y/o su macro estén configurados. El postprocesado conjunto asume que el procedimiento de cambio devuelve el estado y la posición de trabajo adecuados; no lo puede verificar. La simulación física multiherramienta sigue pendiente.

Los bloques personalizados Start/End siguen rechazándose para NC conjunto porque su efecto geométrico no está modelado. En NC individual se conserva el tratamiento existente.
# LaguCAM — reglas para continuar el desarrollo

Estas reglas se aplican a cualquier chat nuevo. La autoridad sobre alcance, aceptación, ejecución en máquina y publicación del proyecto corresponde al usuario.

## Antes de proponer o aplicar cambios

1. Leer [`docs/CONTEXTO.md`](docs/CONTEXTO.md) y [`docs/RESPONSABILIDADES_CAM_NC.md`](docs/RESPONSABILIDADES_CAM_NC.md).
2. Revisar **los archivos reales** que se modificarán. El SHA publicado no garantiza que la copia local de otro ordenador sea idéntica.
3. Distinguir siempre **observado en código**, **validado mediante pruebas** y **pendiente**. No inventar pruebas, garantías, límites ni capacidades CNC.
4. Respetar los contratos existentes y evitar rediseños innecesarios. El usuario aprueba las decisiones de alcance.

## Entrega de modificaciones

- Por defecto, entregar **un parche autocontenido `.ps1` descargable**. El usuario lo ejecuta, valida y realiza sus operaciones Git.
- Resolver la ubicación del proyecto a partir de **`$HOME\Desktop\LaguCAM`**. No usar nombres de usuario fijos, rutas absolutas de un ordenador, ni depender del directorio de trabajo de la consola.
- El parche debe ofrecer **`-CheckOnly`** sin escritura, detectar diferencias y rechazar estados desconocidos. Incluir **`-Restore`** cuando modifique archivos, con copias de rescate **fuera del repositorio** para que no acaben versionadas por accidente.
- No alterar STL, proyectos `.lagucam`, configuraciones personales ni otros archivos ajenos al cambio. No ejecutar comandos Git dentro del parche ni modificar la política de ejecución permanente.
- Entregar un bloque de ejecución para **PowerShell 7**, encadenado con `&&`, usando `pwsh -NoProfile -ExecutionPolicy Bypass -File`.
- Compilar y probar lo que sea aplicable; un cambio solo documental no necesita ejecutar Cargo. Informar de errores reales y de lo que no se pudo validar.

## Principios técnicos

- La malla STL original y los cálculos CAM son independientes de la malla simplificada del visor.
- La estrategia produce trayectorias neutrales; el planificador crea los enlaces entre operaciones; el postprocesador genera G-code; el intérprete del **NC definitivo** alimenta verificación y simulación.
- La simulación Heightfield es **orientativa**: no verifica colisiones, fijaciones, portaherramientas ni seguridad de máquina.
- La interfaz comunica errores reales, advertencias e información sin confirmaciones de riesgo obligatorias para generar NC.
- Documentar únicamente **el estado y el objetivo vigentes**, no el historial de iteraciones. Git conserva la historia.

## Relevo a otro chat

Al empezar un chat nuevo, aportar el repositorio y, si procede, el SHA actual. La nueva sesión debe comenzar por `docs/CONTEXTO.md`, inspeccionar los archivos afectados y comprobar las diferencias respecto a la copia local antes de generar parches.

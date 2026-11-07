# Arquitectura de LaguCAM

## Principios

1. El nucleo CAM no debe depender de Qt.
2. La GUI (Qt 6 Widgets) debe consumir interfaces estables del nucleo.
3. Las trayectorias internas no deben depender de un dialecto concreto de G-code.
4. Todas las operaciones se expresaran con unidades y sistemas de coordenadas explicitos.
5. Los algoritmos CAM deben contar con pruebas y casos geometricos reproducibles.

## Modulos previstos

- `core`: documentos, unidades, sistemas de coordenadas y configuracion.
- `geometry`: importacion, representacion y operaciones geometricas.
- `tools`: herramientas y portaherramientas.
- `cam`: estrategias y generacion de trayectorias.
- `simulation`: verificacion de movimientos, colisiones y material restante.
- `post`: postprocesadores y exportacion NC.
- `ui`: interfaz de escritorio Qt.

## Estado

Especificacion inicial. No implica que estos modulos esten ya implementados.

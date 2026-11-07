# LaguCAM

LaguCAM es un proyecto de software CAM (fabricacion asistida por ordenador) libre y multiplataforma para fresado CNC.

## Estado

**Fase F0: fundamentos.** La aplicacion actual es un esqueleto Qt Widgets, todavia sin funcionalidades CAM.

## Objetivos

- Windows, Linux y macOS.
- C++20, Qt 6 Widgets y CMake.
- Arquitectura modular: interfaz, geometria, CAM, simulacion y postprocesadores.
- Funcionamiento local y pruebas automatizadas.

## Compilacion (una vez instalado Qt 6 y CMake)

```sh
cmake -S . -B build -DCMAKE_PREFIX_PATH="RUTA_A_QT_6"
cmake --build build --config Debug
```

En Qt Creator se puede abrir `CMakeLists.txt` y seleccionar un kit de Qt 6.

## Licencia

**Pendiente de decision formal.** Se propone GPL-3.0-or-later, previa verificacion de compatibilidad con dependencias.
No asumir que el repositorio tiene una licencia libre concedida hasta incluir `LICENSE`.

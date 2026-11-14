# Visión y alcance de LaguCAM

**Estado:** visión acordada · **Revisión:** 2026-11-14

## 1. Propósito

LaguCAM es un proyecto de **software CAM libre** para generar y verificar estrategias de fresado CNC. Comienza con mecanizado de tres ejes sobre modelos STL, pero su diseño debe permitir ampliar formatos geométricos, estrategias y capacidades de máquina.

La motivación es disponer de una alternativa abierta al flujo actual con DeskProto, empezando por trabajos personales de **relieve topográfico en madera** y sin limitar el futuro producto a esa aplicación.

## 2. Caso de uso fundador

- Modelos de terreno creados desde datos de elevación con **QGIS y DEMto3D**.
- Mallas STL binarias de gran tamaño; se han mencionado modelos cercanos a **1 GB** y alrededor de **20 millones de triángulos**.
- Mecanizado inicial en fresadora de **3 ejes** con herramientas planas de aproximadamente Ø6–12 mm para desbaste, esféricas Ø4–6 mm para semiacabado y herramientas cónicas de punta fina para acabado. Son ejemplos de uso, no límites del catálogo.
- Operaciones previstas: desbaste, semiacabado, acabado y contorneado.
- Visualización, definición del bruto y origen, preparación de operaciones, cálculo de trayectorias, verificación y salida G-code.

La geometría de punta de las fresas cónicas reales debe identificarse individualmente: una punta de 0,1 mm **no define por sí sola** si es plana o esférica.

## 3. Principios no negociables

1. **Seguridad CNC y exactitud geométrica verificable:** nada de código ejecutable sobre una máquina sin controles y validación adecuadas.
2. **Robustez y mantenimiento:** claridad de módulos, pruebas, recuperación controlada de errores y regresiones.
3. **Independencia:** el motor CAM no dependerá innecesariamente de egui, OpenGL, SolidCraft, FluidNC ni de un formato de archivo particular.
4. **Rendimiento en mallas grandes:** el visor ya ha demostrado el objetivo inicial en el equipo del usuario; mantenerlo como regresión de referencia.
5. **Tolerancias explícitas:** los cálculos aproximados solo serán aceptables con cotas de error entendidas y verificables.
6. **Modularidad:** geometría, estrategias, herramientas, máquina, postprocesador y presentación tienen responsabilidades diferentes.
7. **Evolución deliberada:** primero un caso de uso de 3 ejes realmente útil y seguro; sin desarrollar prematuramente cinemática de 4/5 ejes.
8. **Software libre y licencias revisables:** proyecto MIT; cualquier dependencia debe evaluarse antes de integrarse, particularmente si limita la reutilización futura.

## 4. Qué es y qué no es LaguCAM

**Es:** una aplicación independiente con camino hacia bibliotecas reutilizables; un sistema de preparación de trabajo y cálculo de trayectorias; un proyecto abierto que debe poder evolucionar.

**No es por ahora:** un sistema CAD paramétrico, un sustituto de un control CNC, un simulador de seguridad certificada ni un programa universal de 5 ejes.

La posible integración futura con SolidCraft **no condiciona el desarrollo actual**. La prueba de un STL grande falló en SolidCraft y funcionó en LaguCAM, pero ello no demuestra que SolidCraft sea inútil para otros casos ni que LaguCAM sea ya un motor CAM.

## 5. Resultado de largo plazo

Permitir seleccionar geometría, bruto, sistema de trabajo, herramientas, máquina y operaciones; calcular trayectorias verificables; obtener G-code compatible con el control seleccionado; conservar un proyecto reproducible y volver a editarlo sin pérdida de configuración.

**El éxito no se mide por cantidad de pantallas, sino por trabajos reales repetibles con resultados comprobables.**

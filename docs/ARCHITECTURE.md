# Arquitectura de LaguCAM

**Fotografía del código:** commit `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`, rama `main` al 2026-11-14.  
**Leyenda:** [ACTUAL] verificado en archivos; [OBJETIVO] diseño acordado o propuesto, aún no implementado.

## 1. Tecnología [ACTUAL]

- Rust, edición 2024; `rust-version = 1.100`.
- `eframe = 0.37.0` con `glow`, fuentes e integración X11/Wayland; `glam`, `bytemuck` y `rfd` entre dependencias directas.
- OpenGL y VBO para render 3D. La interacción gráfica funciona en el equipo del usuario con STL real de casi 1 GB.
- Licencia del repositorio MIT. Dependencias y obligaciones de redistribución necesitan seguimiento específico.

El `Cargo.toml` aún denomina al paquete `cam-libre-visor`; el nombre de la aplicación y el repositorio es **LaguCAM**. Unificar identificadores es limpieza futura, no bloqueo funcional.

## 2. Módulos observados [ACTUAL]

| Ubicación | Responsabilidad |
| --- | --- |
| `src/stl.rs` | Validación y recorrido del STL binario; lectura en dos pasadas, bounds `f64`, cancelación |
| `src/preview.rs` | Preparación visual por bloques de 65.536 triángulos y transporte por canal acotado |
| `src/render.rs` | Shader, VBO y cámara del modelo; carga GPU en bloques y presentación de malla completa |
| `src/stock_render.rs` | Renderizador independiente del bruto, faces/aristas y estado de profundidad |
| `src/overlay.rs` | Proyección 2D de ejes de origen CNC y puntos clicables |
| `src/project/model.rs` | Referencia de modelo, límites y transformación de visualización |
| `src/project/stock.rs` | Bruto rectangular, tamaño, min y extras por seis caras |
| `src/project/origin.rs` | Origen de trabajo, referencia a bruto/modelo, anclas, offset y rotación |
| `src/project/display.rs` | Apariencia del bruto, color/opacidad/modo |
| `src/project/operations.rs` | Colección ordenada de operaciones, todavía marcadores sin cálculo CAM |
| `src/project/selection.rs` | Selección en árbol lateral |
| `src/project/mod.rs` | Agregación del estado del proyecto |
| `src/project_ui.rs` | Panel lateral y propiedades editables |
| `src/main.rs` | Ciclo de vida de la aplicación, importación en hilo, interacción y callback de dibujo |
| `examples/terrain_demo.stl` | STL pequeño para pruebas |

No se observan módulos de motor CAM, biblioteca de herramientas, máquina, postprocesador, serialización de proyecto ni exportación G-code en esta fotografía.

## 3. Flujo de STL grande [ACTUAL]

```text
STL binario en disco (fuente sin modificar)
            │
            ├─ primera pasada → validar y obtener límites en f64
            │
            └─ segunda pasada → triángulos
                               │
                         bloques visuales CPU
                               │
                       canal acotado (2)
                               │
                 transferencia gradual a VBO GPU
                               │
                    render STL por bloques
                               │
                 render bruto con profundidad
                               │
                 overlay CNC y controles egui
```

El lector exige un STL binario consistente con `84 + 50 × N` bytes y coordenadas finitas. No determina por sí mismo que una pieza sea cerrada, mecanizable o libre de autointersecciones.

La representación visual **contiene todos los triángulos** del STL procesado, sin LOD ni decimación en el commit de referencia. No inferir que «streaming» equivale a VRAM ilimitada: el consumo de GPU crece con el número de caras y depende del controlador. La prueba de casi 1 GB es evidencia de viabilidad en el equipo del usuario, no un límite universal garantizado.

El modelo se centra y escala para dibujarlo en `f32`; los límites y transformaciones del estado del proyecto se calculan en `f64`. **La geometría visual no sirve como fuente de verdad para el CAM.**

## 4. Bruto y origen [ACTUAL]

El bruto es un prisma rectangular **alineado con ejes del modelo**, definido por `min: [f64;3]`, `size: [f64;3]` y visibilidad. Puede obtenerse de los bounds del modelo o de extras positivos independientes por X−, X+, Y−, Y+, Z− y Z+.

El cero de trabajo permite anclar a la caja del bruto o la caja envolvente del STL, escoger ocho esquinas o centros, añadir `offset: [f64;3]` en coordenadas del modelo y orientar su base según la convención:

```text
R = Rz · Ry · Rx
p_modelo = origen_modelo + R · p_trabajo
p_trabajo = Rᵀ · (p_modelo − origen_modelo)
```

Se ha programado esta transformación matemática. **No debe confundirse con capacidad física de inclinar la herramienta**: para 3 ejes se mecaniza con eje de husillo fijo y se debe verificar la concordancia entre setup, colocación y control.

La caja del bruto se dibuja en OpenGL tras la malla, con prueba de profundidad. Los indicadores del cero y los puntos de selección son overlays 2D y no resuelven todavía todas las oclusiones.

## 5. Capas futuras [OBJETIVO]

```text
                       LaguCAM UI (egui)
                              │
             Estado persistente del proyecto
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
   Definiciones CAM    Librerías de recursos   Configuración setup
   y operaciones       fresas / máquinas /    origen / bruto /
                      postprocesadores       sujeción / unidades
        │                     │                     │
        └────────────┬────────┴─────────────────────┘
                     │
          Estrategias de mecanizado 3 ejes
                     │
            Servicios geométricos comunes
             ┌───────┴────────┐
          Mallas STL       B-Rep/otros
          (primero)          (futuro)
                     │
               Trayectoria neutral
                     │
         Verificación y comprobaciones
                     │
        Postprocesador (FluidNC primero)
                     │
                 G-code de salida
```

No todos los tipos de geometría deben proporcionar las mismas consultas. Se asignarán estrategias por **capacidades disponibles**, evitando forzar una conversión universal a malla cuando el futuro formato permita superficies exactas.

## 6. Responsabilidades que no deben mezclarse [OBJETIVO]

- **Geometría:** representación original, consultas espaciales, contacto, intersección, tolerancias.
- **Herramienta:** sólidos cortantes y no cortantes, geometría, metadatos, parámetros recomendados; sin número T intrínseco.
- **Operación:** estrategia, stock, setup, selección de herramienta, parámetros, estado y resultados.
- **Trayectoria neutral:** posiciones XYZ; reservar orientación por punto si un caso futuro la exige, sin construir hoy un sistema 5 ejes.
- **Máquina:** límites físicos, husillo, desplazamientos y capacidad de cambio de herramientas.
- **Postprocesador:** dialecto de G-code y secuencias controladas (arranque, parada, retract, cambio, final).
- **UI/render:** visualización e interacción, nunca autoridad sobre coordenadas CAM.

## 7. Invariantes y riesgos

1. La importación de un nuevo STL reinicializa hoy bruto/origen/operaciones (`Project::loaded`). **La persistencia debe restaurar el proyecto sin que esa rutina sobrescriba el estado guardado.**
2. Las rutas del STL no se almacenan de modo persistente en el estado actual; la persistencia necesita un identificador/ruta robustos y gestión de fichero ausente.
3. La biblioteca global y los snapshots del proyecto deben evitar que una modificación externa altere silenciosamente una trayectoria.
4. Los datos de representación visual nunca deben alimentar cálculos G-code sin una especificación de error y validación.
5. El controlador FluidNC no está implementado; no hay salida G-code operativa validada.
6. Antes de implementar estrategias, definir operaciones y pruebas geométricas que eviten movimientos peligrosos incluso con STL corruptos o incompletos.

## 8. Referencias de código

[Árbol del repositorio en el commit base](https://github.com/aik3n/LaguCAM/tree/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src) · [`src/project/origin.rs`](https://github.com/aik3n/LaguCAM/blob/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src/project/origin.rs) · [`src/preview.rs`](https://github.com/aik3n/LaguCAM/blob/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src/preview.rs)

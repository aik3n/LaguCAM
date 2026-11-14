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

En esta entrega se incorpora biblioteca de herramientas y la serialización de proyectos ya existe en el código actual. No existe motor CAM, biblioteca de máquinas, postprocesador ni exportación G-code.

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
3. Las operaciones futuras resolverán su herramienta por UUID en la biblioteca global. Un UUID ausente bloqueará la operación; cualquier cambio de definición invalidará resultados previamente calculados. No se guardarán snapshots ocultos.
4. Los datos de representación visual nunca deben alimentar cálculos G-code sin una especificación de error y validación.
5. El controlador FluidNC no está implementado; no hay salida G-code operativa validada.
6. Antes de implementar estrategias, definir operaciones y pruebas geométricas que eviten movimientos peligrosos incluso con STL corruptos o incompletos.

## 8. Referencias de código

[Árbol del repositorio en el commit base](https://github.com/aik3n/LaguCAM/tree/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src) · [`src/project/origin.rs`](https://github.com/aik3n/LaguCAM/blob/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src/project/origin.rs) · [`src/preview.rs`](https://github.com/aik3n/LaguCAM/blob/0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9/src/preview.rs)

## Biblioteca global de fresas (módulos de la nueva entrega)

```text
src/tools/model.rs       Tool, ToolGeometry, ToolLibrary; validación f64, perfil axial, IDs y revisiones
src/tools/storage.rs     JSON global versionado en configuración de usuario; escritura temporal y sustitución
src/tools_ui.rs          Ventana egui de tres columnas, búsqueda, editor de borradores y confirmaciones
src/tools_profile.rs     Proyección visual de la sección axial; cotas; no altera geometría CAM
```

La biblioteca no es miembro de `Project`, no entra en la serialización `.lagucam` y no se enlaza con operaciones. El modelo geométrico no depende de egui; la vista 2D recibe un perfil validado.

No hay cambios en `src/stl.rs`, `src/preview.rs`, `src/render.rs`, `src/stock_render.rs` ni `src/overlay.rs`. El estado de herramientas permanece fuera de GPU y de los hilos de importación.

## Ajuste 009 — Biblioteca y cielo

- La biblioteca `App::tool_library` sigue siendo **global** y existe sin STL/proyecto. Solo se abre desde `Herramientas -> Biblioteca de herramientas` en el menú principal.
- Se elimina el acceso a la biblioteca desde el árbol y la variante obsoleta `Selection::Tools`, sin modificar los archivos `.lagucam` porque la selección visual no se persiste. Las operaciones todavía no asignan herramientas; cuando se implementen guardarán el UUID global sin copiar propiedades.
- El degradado del viewport se dibuja como `egui::Mesh` de cuatro vértices, **antes** del callback OpenGL. En `Render3d::paint` se limpia solo `DEPTH_BUFFER_BIT`; se conserva la carga GPU por bloques y la profundidad para malla y bruto. Requiere prueba gráfica y con STL grande en Windows.

## Implementación 010 — Navegación por pivote genérico

- `src/navigation.rs` define la matriz de cámara orbital y la desproyección en coordenadas visuales, sin depender del formato STL/STEP/IGES ni de egui.
- `src/scene_pick.rs` consulta la profundidad del renderizado de la escena en un único píxel mediante un FBO 1×1 para resolver MSAA. No almacena duplicados de geometría ni abre el STL del disco.
- `src/main.rs` registra el doble clic y comunica el resultado del callback GL al siguiente frame. Solo cambia el pivote de cámara; no modifica el origen CNC, las coordenadas de geometría ni `.lagucam`.
- La primera superficie seleccionable es la malla que produce el renderizador actual; futuros backends podrán aportar profundidad al mismo mecanismo de escena sin cambios en la navegación. La lectura de profundidad es para **interacción visual**, no para cálculo CAM.
- Pivote inicial y `Vista > Centrar vista`: centro de bounds del modelo; si no hay hit o el driver no permite el readback, se conserva el anterior. El arrastre derecho desplaza vista/pivote como hasta ahora.
- Pendiente validación en GPU real, incluida muestra única/MSAA, bruto y malla de ~1 GB.

## Ajuste 011 — Pivote por clic sin salto

- `src/navigation.rs` mantiene separados el centro de la vista (`target`) y el pivote orbital seleccionado. Elegir un punto no cambia la matriz de cámara ni desplaza la vista.
- En el siguiente arrastre, el centro de cámara se transforma en torno al pivote mediante las bases ortonormales de las orientaciones anterior y nueva. El punto elegido permanece estable en pantalla mientras gira.
- El marcador se proyecta sobre su posicion real en pantalla, no sobre el centro del viewport. Un clic sin geometria conserva la seleccion anterior.
- No hay dependencia del formato: la escena aporta la profundidad visible. El modo Origen CNC mantiene prioridad para elegir sus puntos rojos.

## Ajuste 012 — Pulsar y arrastrar en un solo gesto

- La UI detecta la pulsacion primaria al comienzo del gesto, no el evento `clicked()` de liberacion. Se solicita una lectura de profundidad en el callback GL de la escena.
- `PivotPress` acumula desplazamientos mientras esta pendiente la consulta: el primer arrastre no gira alrededor de un pivote obsoleto. Tras recibir la respuesta, se fija el pivote y se aplica el desplazamiento acumulado.
- Un clic sin arrastre cambia solo el pivote y deja la vista intacta. El marcador queda asociado al pivote y NO depende del boton pulsado. En el fondo se conserva el anterior.
- La navegacion no conoce STL/STEP/IGES; los modelos proporcionan superficies a traves de la representacion visible de la escena. El modo Origen CNC conserva la prioridad de sus puntos rojos.

## Ajuste 013 — Visibilidad del pivote y acciones configurables

- `PivotPress` diferencia el pivote persistente de la visibilidad temporal de su indicador. Soltar la accion de orbita oculta la marca, pero NO restablece ni modifica el pivote.
- `NavigationBindings` centraliza la asignacion de la accion orbital y de desplazamiento (boton izquierdo para orbitar, derecho/central para desplazar). Son ajustes por defecto, no una interfaz de configuracion todavia.
- La marca solamente se dibuja mientras esta activo el gesto y la consulta de profundidad ha resuelto el punto. La vista y las formulas de camara no cambian.
- El binding se mantiene ajeno al formato del archivo. STL/STEP/IGES son proveedores de superficies de la escena, no tipos de camara.

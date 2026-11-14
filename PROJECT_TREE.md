# LaguCAM - árbol de proyecto / bruto / origen CNC

Esta actualización añade un árbol lateral y un estado de proyecto en módulos Rust separados.
El STL sigue cargándose por bloques sin modificación de stl.rs, preview.rs ni render.rs.

- El bruto es una caja rectangular alineada con los ejes del modelo; `min` expresa su esquina mínima en coordenadas STL y `size` contiene dimensiones reales f64.
- El botón Ajustar a límites del STL utiliza los límites originales. Un eje del STL con extensión cero se inicializa con grosor 1 unidad para obtener un bruto válido.
- El origen de trabajo se define como desplazamiento desde la esquina mínima del bruto. Los presets incluyen esquinas, centro superior y centro del bruto.
- El origen personalizado admite posiciones fuera del bruto intencionadamente.
- La representación del bruto es un dibujo de aristas proyectado con la cámara 3D; es una superposición sin prueba de profundidad, por lo que las aristas pueden verse delante del STL.
- No se persisten los datos al cerrar, no hay algoritmos CAM, herramientas funcionales, trayectorias ni G-code.
- La geometría de referencia y todas las posiciones/dimensiones se guardan en f64; la conversión a f32 solo ocurre durante la proyección visual.
- Las operaciones usan una colección Vec ordenada, inicialmente vacía.

## Validación

El script de instalación ejecuta cargo fmt, clippy, test y build --release. Validar visualmente la caja, los ejes, el selector de origen y un STL grande en el equipo con GPU.
# LaguCAM 005: aspecto del bruto

Base: commit `016c618957f5b4a25edfb906bd20441ae7dfae6f`.

- Bruto: tres modos (sólido translúcido, solo aristas, ambos), cuatro colores y opacidad de 0 a 100 %. Valor inicial: 25 % ámbar, sólido y aristas.
- El visor dibuja primero el STL opaco, luego las caras visibles y las aristas del bruto con prueba de profundidad contra el STL. Las zonas tapadas por la malla quedan ocultas.
- El sólido utiliza las caras exteriores orientadas a cámara (back-face culling) y mezcla alfa, sin escribir profundidad, para evitar que las caras traseras se superpongan como otra capa translúcida.
- El bruto respeta los límites del modelo en f64. Solo los vértices enviados para dibujar se normalizan a f32. No altera STL, CAM ni sistema de coordenadas CNC.
- `src/stl.rs`, `src/preview.rs` y `src/render.rs` quedan intactos; el renderizador auxiliar dispone de su propio programa, VAO y VBO pequeño.

## Limitaciones

- El indicador XYZ del origen CNC sigue dibujándose en 2D encima de la vista, sin ocultación por profundidad.
- Las aristas del bruto se ocultan detrás del STL, pero pueden verse a través de las caras translúcidas del propio bruto.
- Con brillo de caras coplanares con el STL puede aparecer z-fighting; la profundidad de la malla es la autoridad.
- La visualización usa un prisma alineado a ejes. El STL no define unidades. No hay persistencia del proyecto en disco.
- Es necesario verificar visualmente la ocultación, los controles y el rendimiento con STL grandes en el equipo final.

Validación automatizada: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `cargo build --locked --release`.
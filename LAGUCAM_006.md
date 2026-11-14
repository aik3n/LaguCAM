# LaguCAM 006 — Selección visual del cero CNC y color del modelo

Base validada por firma de archivos: commit GitHub `c482baaa8b3737ef9cbf5484171331c4c10447e4`.

- Origen CNC: referencia a bruto o caja envolvente del STL, ocho esquinas y centros superior/general para cada referencia.
- Al seleccionar **Origen CNC**, aparecen puntos rojos de 8 píxeles de diámetro, ampliados a 12 píxeles al pasar el cursor. Se identifican con texto y se eligen con clic. Los marcadores se ocultan al cambiar de sección.
- El selector en propiedades permite elegir explícitamente la referencia cuando coinciden puntos del bruto y del modelo.
- Offset XYZ independiente de la rotación CNC. Se calcula en coordenadas f64 originales del STL. Un clic sobre un marcador cambia la referencia y restablece el offset a 0.
- Se mantienen la rotación Euler Rz · Ry · Rx y la visualización del sistema de coordenadas CNC.
- Color del modelo configurable desde **Modelo STL → Propiedades** con selector RGB. El STL permanece opaco y con iluminación.
- Se mantiene intacta la importación en streaming y por bloques de STL grandes, los datos geométricos, la representación del bruto de la versión 005 y el sistema de transferencia VBO.

## Verificación

El instalador ejecuta `cargo fmt --all -- --check` antes de intervenir, genera copias de seguridad, y después aplica `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` y `cargo build --locked --release`. Se restauran los archivos si algún comando falla. No se ejecutan comandos Git.

La compilación y la prueba OpenGL real deben completarse en Windows; este entorno no incluye Rust/Cargo.
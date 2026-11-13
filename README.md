# CAM Libre — Visor STL 0.1, primera revisión auditada

Aplicación de escritorio en Rust + egui/eframe + OpenGL. **Base compilada y probada; la versión 0.1 NO está terminada.** La visualización interactiva en un escritorio real y el objetivo de 700 MB–1 GB siguen pendientes. Véase `AUDITORIA.md`.

## Compilación y ejecución

Instala Rust mediante <https://rustup.rs/>. `rust-toolchain.toml` fija Rust 1.100.0, versión utilizada en esta revisión; `Cargo.lock` fija las dependencias. Desde el directorio del proyecto:

```bash
cargo build --locked --release
cargo run --locked --release -- examples/terrain_demo.stl
# O abrir el selector de archivos:
cargo run --locked --release
```

El ejecutable se genera en `target/release/cam-libre-visor` (Windows: `.exe`). Se necesita un escritorio con OpenGL 3.3 y soporte de profundidad. La compilación de Windows/macOS no se ha comprobado. No se incluye un binario portable.

En Ubuntu/Debian, si faltan bibliotecas de desarrollo:

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libwayland-dev libegl1-mesa-dev libgl1-mesa-dev
```

El selector de archivos Linux de `rfd` utiliza un portal de escritorio con sus características actuales. Si no está disponible, se puede abrir el archivo por argumento. Sin servidor gráfico la aplicación no puede arrancar.

## Menús

- **Archivo → Abrir STL…**: abre el selector de archivos STL. También hay un botón lateral.
- **Vista → Centrar vista**: restablece orientación, zoom y desplazamiento para encuadrar la pieza completa.

## Controles

- Botón izquierdo y arrastre: rotación orbital.
- Rueda: zoom, cuando el cursor está sobre la vista.
- Botón derecho o central y arrastre: desplazamiento de cámara.
- **Centrar vista**: restablece orientación, desplazamiento y encuadre.
- **Cancelar importación**: interrumpe la lectura en segundo plano. Conserva la vista anterior.

## Datos y límites explícitos

Solo STL **binario estándar**, no ASCII. Se exige tamaño exacto `84 + 50 × triángulos`, al menos un triángulo y posiciones finitas. Se admiten planos verticales. Una geometría con extensión nula en todos los ejes se rechaza. No se valida que la malla sea cerrada, manifold o apta para fabricar.

El lector recorre el archivo dos veces con un búfer de 4 MiB: primero calcula límites en f64 y luego construye la vista. No crea una segunda colección de triángulos originales. El STL fuente se abre exclusivamente para lectura y permanece en disco, intacto. No hay todavía almacén geométrico persistente ni caché para CAM; no borrar ni modificar el archivo fuente durante la importación.

La vista conserva **todos los triángulos**: sin heightfield, submuestreo, relleno de huecos ni simplificación. Solo centra y escala uniformemente las posiciones para la GPU. El cálculo de esa transformación usa f64; los vértices GPU son f32 y no son una representación apta para CAM. Detalles muy pequeños respecto a la extensión total pueden perder precisión visual. Las normales se recalculan para iluminación, sin modificar el archivo. Los triángulos degenerados se conservan, pero no tienen superficie visible.

**Límite actual:** 256 MiB de datos CPU de la malla visual, a 72 bytes por triángulo (tres posiciones y normales). Equivale a 3.728.270 triángulos o aproximadamente 177,8 MiB de STL. Una carga superior se rechaza íntegramente; nunca se muestra una selección incompleta como si fuera la pieza entera. Este límite es una protección provisional y **no satisface el objetivo de 1 GB**.

El presupuesto no limita la memoria total del proceso: además hay búfer de lectura, GUI, malla anterior en GPU y memoria del controlador. La carga a GPU todavía es monolítica y puede causar una pausa al terminar la importación. La cancelación es cooperativa entre registros; no interrumpe una lectura del sistema operativo bloqueada ni una subida GPU. El tiempo mostrado corresponde a lectura y construcción CPU, no al tiempo total hasta el primer fotograma.

## Pruebas

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

La CI ejecuta estos controles. No ejecuta una prueba gráfica ni mide rendimiento. Hay 13 pruebas de lectura, errores, cancelación en ambas pasadas, límites de memoria, transformación visual, conservación de caras y encuadre. El ejemplo incluido se carga en una prueba y debe conservar sus 32.768 triángulos.

## Arquitectura

| Archivo | Responsabilidad |
| --- | --- |
| `src/stl.rs` | E/S binaria progresiva, validación, bounds f64, cancelación y callback por triángulo |
| `src/preview.rs` | Única malla visual CPU, transformación y presupuesto de memoria |
| `src/render.rs` | Recursos OpenGL, VBO intercalado, shaders, profundidad y encuadre |
| `src/main.rs` | Interfaz, canal de resultados, hilo de importación y controles de cámara |

La vista no alimentará futuros cálculos CAM. La separación de responsabilidades existente se conserva. No se han añadido estrategias CAM, herramientas ni G-code.

## Validación pendiente en tu equipo

1. Compilar y abrir el ejemplo. Comprobar orientación, oclusión, encuadre, giro, zoom y desplazamiento; también en una ventana estrecha.
2. Abrir otro archivo inválido y cancelar una importación: debe seguir visible el modelo anterior con su nombre y datos.
3. Comprobar el selector de archivos y los errores de contexto/GPU.
4. Antes de la aceptación de 0.1, implementar y validar el camino de grandes mallas descrito en la auditoría. Esta revisión rechazará un STL de 700 MB–1 GB.
5. Una vez disponible ese camino, medir con tu montaña real: CPU/GPU/RAM/VRAM, disco, tamaño y triángulos, tiempo hasta el primer fotograma y hasta carga completa, memoria máxima, tiempos de fotograma al girar/zoom/desplazar y latencia de cancelación. No se presume validación con un archivo sintético.

## Licencias

Código del proyecto: MIT (`LICENSE`). No se han añadido dependencias. Las cuatro dependencias directas ofrecen licencias permisivas. `DEPENDENCIAS.md` incluye el inventario de expresiones de licencia descargadas en Linux; no constituye una aprobación de integración con SolidCraft ni una revisión completa de todas las plataformas. Antes de distribuir binarios, recopilar avisos y licencias de las dependencias, incluidas las fuentes tipográficas.

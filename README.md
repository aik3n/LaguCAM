# LaguCAM

**CAM libre experimental para fresado CNC.** Estado del código documentado: [`f1e436b976eb6f4f88482b00ae5aeff2761e08ee`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee), 14 de noviembre de 2026. Licencia MIT.

LaguCAM nace para mecanizar relieves topográficos a partir de STL binarios enormes, pero se diseña para piezas, formatos y máquinas diferentes. **Incluye una estrategia CAM experimental de acabado paralelo con previsualización 3D, pero no exporta G-code.** No constituye un sistema de mecanizado verificado.

## Funcionalidad disponible

- Motor geométrico STL independiente del visor, BVH y consultas de contacto para fresa esférica (dentro de la precisión de la malla).
- Estrategia de prueba «Acabado paralelo», con pasos, dirección, diámetro y zona XY; solo previsualización, sin entradas, retiradas ni programa CNC.
- Estado y limitaciones del prototipo: [`docs/CAM_INTEGRACION_2026-11-15.md`](docs/CAM_INTEGRACION_2026-11-15.md).

- Lector STL binario en dos pasadas, importación y transferencia a OpenGL por bloques, sin modificar el STL fuente.
- Visor con rotación, zoom, desplazamiento y pivote visual seleccionable en la superficie visible.
- **Validación comunicada por el usuario:** apertura y navegación fluida de un STL real de casi 1 GB en su equipo; no es un benchmark cuantitativo ni garantía para todo hardware.
- Proyecto con árbol, bruto rectangular editable, extras en seis caras, colores y origen CNC anclado a bruto o modelo, con offsets y rotación matemática.
- Guardar y abrir proyectos `.lagucam` **versión 2**, JSON con referencias externas al STL y unidades en milímetros.
- Biblioteca global de herramientas con cinco familias geométricas, filtros, editor y perfil técnico 2D; identificadores UUID y revisiones.
- Paneles Proyecto/Propiedades acoplables o flotantes, temas claro/oscuro y preferencias portables.
- Operaciones CAM experimentales con parámetros independientes, editor por pestañas y guardado en `.lagucam`; selección desde la biblioteca de herramientas todavía pendiente.

## Compilación y uso

Rust edición 2024, versión fijada en `rust-toolchain.toml` (1.100), egui/eframe y OpenGL/glow. Es necesario un entorno gráfico y GPU compatibles. El paquete Cargo sigue llamándose `cam-libre-visor`.

```bash
cargo build --locked --release
cargo run --locked --release
cargo run --locked --release -- examples/terrain_demo.stl
```

El ejecutable aparece en `target/release/cam-libre-visor` (`.exe` en Windows). Los menús ofrecen **Archivo** (Nuevo/Abrir/Guardar proyecto, Abrir STL), **Herramientas** (Biblioteca de herramientas y Opciones) y **Vista** (Centrar vista, Restablecer paneles).

**Controles actuales:** mantener el botón izquierdo sobre el modelo y arrastrar para orbitar desde un pivote visual elegido en la pulsación; rueda para zoom; botón derecho o central para desplazar. El indicador del pivote solo aparece durante el gesto de órbita. Los paneles se reorganizan mediante su asa de arrastre.

## Configuración y datos

- La distribución portable guarda `config/tools.json`, `config/panels.json` y `config/settings.json` **junto al ejecutable**. Al ejecutar desde `target/release`, los datos quedan junto al binario de esa carpeta. No se usa AppData ni el Registro.
- El archivo `.lagucam` v2 almacena una referencia al STL, no los triángulos. El proyecto no guarda una copia de la definición de cada herramienta: las operaciones referenciarán su UUID global.
- El esquema `.lagucam` v1 se rechaza. **Durante el desarrollo no se exige compatibilidad con formatos antiguos ni su migración**; no se deben borrar datos personales silenciosamente.
- STL no contiene unidades, aunque el proyecto actual trabaja en mm. Los datos geométricos usan `f64`; el dibujo GPU normalizado usa `f32` **y no es una base válida para cálculo CAM**.
- Solo STL **binario**. Se comprueba la estructura, pero no que la malla sea manifold, cerrada o apta para mecanizar. GPU, RAM y rendimiento dependen del tamaño de entrada y del hardware.

## Próxima etapa acordada

El commit de referencia unifica el tipo `ToolId` (UUID textual) en biblioteca, operaciones y persistencia. **Está acordado —pero no implementado— calcular una huella SHA-256 de los parámetros geométricos de las fresas** mediante codificación binaria canónica y versionada. Excluirá UUID, nombre, revisión, notas, RPM y avances. Aún no existen trayectorias ni reglas ejecutables para comprobar su vigencia.

## Pruebas y límites

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

Son comandos recomendados, **no pruebas ejecutadas en esta actualización documental**. No se ha constatado un workflow de CI activo en el árbol revisado. Ningún modelo mostrado, ni futuro G-code, debe considerarse seguro para mecanizar sin verificación geométrica, de offsets, herramienta física, colisiones y máquina real.

## Documentación

Leer [AGENTS.md](AGENTS.md) y [docs/INDEX.md](docs/INDEX.md). Estado real: [docs/STATUS.md](docs/STATUS.md). Arquitectura: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Decisiones: [docs/DECISIONS.md](docs/DECISIONS.md). Bibliotecas: [docs/LIBRARIES.md](docs/LIBRARIES.md). Proceso de trabajo: [docs/WORKFLOW.md](docs/WORKFLOW.md).

Las entregas `LAGUCAM_004.md` a `LAGUCAM_007.md`, `AUDITORIA.md` y `DEPENDENCIAS.md` son documentos de etapas anteriores y deben interpretarse como históricos.

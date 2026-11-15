# LaguCAM — integración CAM experimental (15-11-2026)

## Identificación

- Repositorio local: `Desktop\LaguCAM`.
- Rama: `integracion/cam-20261115`.
- Commit base probado: `7de83c9` (15 archivos de código integrados; GitHub `main` todavía sin actualizar).
- Copia histórica de experimentación: `Desktop\LaguCAM-G13-pruebas`; ya no es el directorio de desarrollo activo.

## Estado implementado

- Motor de consultas del STL binario en `src/cam_geometry/`: índices BVH, lectura por bloques, presupuesto de memoria y contacto con una fresa esférica. El cálculo usa coordenadas de los triángulos originales, independientemente de la malla GPU.
- Operaciones por proyecto: crear una operación sin estrategia, seleccionar con un clic, editar mediante doble clic en ventana única (General, Estrategia, Área), eliminar desde el editor, guardar configuraciones en `.lagucam`.
- Estrategia experimental «Acabado paralelo — TEST»: diámetro esférico, separación lateral, paso longitudinal, X/Y y límites opcionales XY; generación en segundo plano con cancelación y límite de 12.000 puntos. Los resultados visualizados son *muestras de contacto*, no una trayectoria CNC completa.
- Pruebas y visualizaciones adicionales: `sample_direct`, `sample_overlay`, `section_overlay`, sección exacta de triángulos; módulos temporales de diagnóstico, no interfaces públicas CAM definitivas.

## Evidencia de validación (Windows del responsable)

- `cargo fmt --all -- --check`: correcto.
- `cargo test --all-targets`: 130 pruebas correctas (20 del módulo de biblioteca, 104 de la aplicación y 6 del contrato G18).
- `cargo check --bin cam-libre-visor` y `cargo build --release`: correctos, con cuatro avisos no bloqueantes en el commit base.
- Prueba del usuario tras integrar: LaguCAM original abre, muestra STL y permite crear, editar y eliminar operaciones.
- Pruebas previas en carpeta experimental: distintas operaciones y parámetros, área XY, generación visual y guardado/apertura de proyecto.

## Limitaciones y siguiente trabajo

- No hay G-code ni postprocesador; no hay simulación de máquina, entradas/retiradas, control de colisiones ni validación integral para ejecutar en una fresadora.
- El muestreo longitudinal no copia continuamente el STL: calcula puntos de contacto según el diámetro y los pasos. La estrategia debe validarse contra geometrías patrón antes de considerarla operativa.
- La herramienta de la biblioteca global aún no está vinculada a la estrategia TEST mediante UUID y verificación de huella geométrica; la invalidación completa de resultados queda pendiente.
- El visor incluye diagnósticos transitorios y las pestañas de operación pueden requerir refinamiento de UX; evitar nuevos menús de pruebas fuera del árbol de operaciones.
- No eliminar archivos de proyecto o STL, ni fusionar con `main` ni publicar en GitHub sin decisión expresa y pruebas posteriores.

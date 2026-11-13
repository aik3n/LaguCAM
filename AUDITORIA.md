# Auditoría inicial — 13 de noviembre de 2026

## Dictamen

La propuesta inicial **compila**, y sus tres pruebas pasan con Rust 1.100.0 en Linux x86_64. No era necesario reescribir la aplicación ni cambiar eframe. Su principal defecto no era de compilación: la conversión incondicional a heightfield 512×512 no representa un STL general y puede ocultar regiones importantes. La primera revisión corrige esa representación y añade controles y pruebas. **No se declara aceptada la versión 0.1.**

## Alcance de la inspección original

Se revisaron los diez archivos del ZIP: Cargo.toml, README, LICENSE, .gitignore, workflow CI, los cuatro módulos Rust y el STL binario de ejemplo. Este último se recorrió con una comprobación independiente en Python: 1.638.484 bytes, 32.768 triángulos, tamaño exacto y posiciones finitas. Límites: mínimo `[-100, -100, -2.043564558029175]`, máximo `[100, 100, 50.252662658691406]`.

SHA-256 del ejemplo, sin modificaciones: `043ae409626e183cab227bda4dcc4f15810f0b8dff8ad3f38cec5d700db0bfa3`.

Dependencias originales: eframe `=0.37.0` con Glow, fuentes, X11 y Wayland; glam `0.30`; bytemuck `1.23`; rfd `0.15`. Se conservan. El lockfile resuelve las directas a 0.37.0, 0.30.10, 1.25.2 y 0.15.4. No había Cargo.lock ni versión fijada de toolchain. La API `App::ui` usada es válida para esta versión de eframe; no se cambió por la API antigua `update`.

## Hallazgos y tratamiento

| Prioridad | Hallazgo original | Tratamiento |
| --- | --- | --- |
| Crítica | Altura Z máxima por XY pierde bases, caras verticales, voladizos y capas superpuestas | Sustituido únicamente el generador visual por todos los triángulos; sin muestreo |
| Alta | Relleno de huecos inventa superficies; porcentaje de celdas no mide fidelidad geométrica | Eliminados relleno y porcentaje |
| Alta | Rechazo de STL planos verticales por ausencia de extensión XY | Se exige extensión en al menos un eje |
| Alta | No hay cancelación | Bandera cooperativa en ambas pasadas; botón, salida y carrera entre resultado/cancelación contemplados |
| Alta | No hay desplazamiento de cámara; distancia fija no encuadra ventanas estrechas | Pan con botón derecho/central; encuadre basado en esfera y aspecto |
| Alta | Nuevo nombre con vieja malla/dimensiones si falla la importación | Nombre pendiente separado; solo se confirma tras éxito CPU |
| Alta | Bounds, centro y normalización f32 pueden desbordar con coordenadas válidas | Operaciones intermedias f64 y regresiones para extremos finitos |
| Media | Panic ante errores de shaders/creación GL | Propagación de errores de inicio y liberación de recursos parciales; detección de errores en subida GPU |
| Media | Limpieza GL puede abarcar el clip del panel completo | Intersección explícita del clip con el viewport; escritura de profundidad explícita |
| Media | Solo tres pruebas, sin casos estructurales/cancelación | Trece pruebas automatizadas |
| Media | CI formatea modificando código y no trata warnings como fallo | fmt --check, clippy -D warnings, comandos --locked |
| Alta, abierta | Camino escalable para 700 MB–1 GB no validado | Presupuesto visual provisional; grandes archivos se rechazan con explicación |
| Alta, abierta | Subida GPU completa en un callback | Pendiente fragmentación y medición; posible pausa final |

Se conserva el lector en dos pasadas, el hilo de importación, canal/progreso atómico, los límites de módulo, egui/eframe, shaders básicos, ejemplo, licencia y pipeline de compilación. Las normales de STL se ignoran para iluminación y se recalculan; los atributos de color no se visualizan. La fuente original no se edita.

## Comprobaciones realmente realizadas

Entorno inicialmente sin Rust; instalado localmente desde la distribución oficial. Rust/Cargo 1.100.0, Linux x86_64. No se ejecutaron pruebas en Windows/macOS ni en el equipo del usuario.

| Comprobación | Resultado |
| --- | --- |
| Original: cargo build | Correcto |
| Original: cargo test | 3/3 correctas |
| Original: cargo clippy --all-targets | Correcto, 1 advertencia collapsible_if |
| Revisado: cargo fmt --check | Correcto |
| Revisado: cargo test --locked | 13/13 correctas |
| Revisado: cargo clippy --locked --all-targets -- -D warnings | Correcto, sin advertencias |
| Revisado: cargo build --locked --release | Correcto |
| Referencia por lector + generador Rust | 32.768 triángulos, 589.824 floats visuales; extensión X 200 |
| Arranque gráfico desde ejecución | Bloqueado: no DISPLAY, WAYLAND_DISPLAY ni WAYLAND_SOCKET |
| Escritorio alternativo | Terminal abierta, sin foco utilizable; visor no ejecutado allí |
| Giro, zoom, pan, oclusión y cancelación con UI | No verificados visualmente |
| STL real de montaña 700 MB–1 GB | No disponible; no probado, no soportado por este presupuesto provisional |

Las pruebas unitarias de cámara solo comprueban el encuadre matemático. Las de cancelación comprueban la interrupción del lector, no latencia de interfaz ni de almacenamiento. Las de memoria fuerzan un presupuesto diminuto, no simulan fallo real del controlador GPU. Los tiempos de compilación no son métricas de rendimiento del visor.

## Estrategia visual y siguiente iteración, todavía dentro de 0.1

Esta revisión no simplifica: conserva las caras y topología de entrada en la representación visual, salvo precisión f32 de las coordenadas normalizadas. Rechaza la carga completa si supera 256 MiB visuales. Es una base de fidelidad para comparar una futura simplificación, no la solución final al objetivo de gran tamaño.

Para 1 GB decimal de STL, la representación actual requiere aproximadamente 1,44 GB de posiciones/normales CPU y otro tanto GPU, además de memoria de driver y aplicación. No basta con levantar el límite: el upload y dibujar alrededor de veinte millones de triángulos requieren medidas reales.

Propuesta para decidir en la siguiente iteración:

1. Separar el transporte de malla en bloques con cola acotada y subida GPU presupuestada por fotograma, con cancelación durante toda la operación. Evitar picos y mantener la GUI operativa también al finalizar.
2. Medir el camino completo sin simplificación; introducir partición espacial solo si la medición exige culling, caché o LOD. Procesar todos los triángulos, independientemente del orden en el STL. Un muestreo cada N triángulos no ofrece cobertura espacial y no se recomienda.
3. Si se requiere LOD, comparar cada bloque con una representación simplificada que preserve contornos/componentes y tenga un error visual explícito y verificable. Acordar antes una tolerancia en unidades del modelo o pantalla. No emplear esa simplificación para CAM ni presentar regiones aún sin cargar como pieza completa. Cualquier vista provisional debe indicar claramente su estado.
4. Validar con STL de referencia que tengan planos verticales, base, huecos, voladizos y componentes separados; después con la montaña real de 700 MB–1 GB, registrando hardware y métricas del README.

No se implementó partición/LOD sin tolerancia ni datos que justifiquen su diseño. La elección de esa estrategia requiere la dirección técnica prevista por el proyecto.

## Ajuste solicitado tras la auditoría

Añadidos menú superior **Archivo → Abrir STL…** y **Vista → Centrar vista**, conservando botones laterales. Centrar restablece orientación, zoom y desplazamiento.

## Riesgos que permanecen

- GL 3.3, MSAA, recursos y selector dependen del escritorio/controlador; no hay ensayo visual ni matriz multiplataforma.
- Detección de modificación del archivo mediante tamaño/mtime es de mejor esfuerzo: no garantiza una instantánea inmutable. El archivo fuente sigue siendo la autoridad; no hay aún un almacén original para CAM.
- La cancelación no puede interrumpir E/S del sistema operativo bloqueada. No se ha medido su latencia en discos lentos.
- GPU: subida monolítica y comprobación básica de errores. Un fallo deja la vista vacía con error; no se garantiza recuperar un contexto perdido. Los errores iniciales se devuelven a eframe, sin diálogo propio de recuperación.
- Selector nativo modal; no bloquea una importación porque no se permite abrirlo mientras se importa. Debe comprobarse su disponibilidad por plataforma.
- La memoria de la vista crece con el número de triángulos hasta el límite. No existe todavía un renderizador de grandes mallas ni garantía de fluidez.
- Las licencias directas son permisivas, pero falta revisión de integración/distribución para SolidCraft y de todas las dependencias por plataforma. No se ha incorporado una dependencia nueva GPL/LGPL.

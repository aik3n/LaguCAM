# LaguCAM 007 — Persistencia de proyectos, etapa 1

Base: `0e56d3f3f4d5022c1ee6a3d78e442d94c37192c9`.

## Formato

`.lagucam` es un JSON UTF-8 con `format: "LaguCAM"`, `version: 1`, `units: "mm"`,
una referencia externa opcional al STL (`path`, bytes, triángulos, límites f64,
firma de cinco muestras y fecha), y `state` con el bruto, visualizaciones, origen
CNC, color del STL y operaciones ordenadas. No contiene triángulos.

La ruta STL se almacena relativa al proyecto cuando comparte volumen; se admite
una ruta absoluta en otros casos. Al abrir se comprueba el tamaño, los triángulos
y un FNV-1a de cinco muestras de hasta 64 KiB (≈320 KiB de lectura). No es un
hash criptográfico: una alteración únicamente entre muestras podría no detectarse
si tampoco cambian los límites. Se vuelven a comparar los límites completos
recalculados por el importador antes de confirmar la nueva malla.

## Seguridad

La configuración guardada solo se instala tras una carga STL correcta. Los datos
RAM, cámara, referencias OpenGL y hilos no se serializan. Un error de ruta, datos,
formato o importación conserva el proyecto anterior. Nuevo/Abrir STL/Abrir
proyecto/Salir advierten de cambios no guardados. La escritura usa un temporal,
`sync_all` y reemplazo con copia de recuperación Windows (`ReplaceFileW`).

## Pruebas y limitaciones

Se incluyen pruebas automáticas de los datos, f64, rutas, identidad STL,
versión, JSON, geometría, escritura fallida y preservación del estado.

La firma muestreada no garantiza la detección criptográfica de todos los cambios;
el formato v1 solamente admite milímetros y una referencia STL. La importación
STL de 1 GB y el comportamiento gráfico deben comprobarse en Windows.

El instalador valida con Cargo y restaura archivos si falla. No crea commits ni
modifica Git automáticamente.
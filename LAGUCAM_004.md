# LaguCAM 004 — extras por cara y rotación del origen CNC

Actualización local sobre LaguCAM 002/003. **No modifica** `src/stl.rs`,
`src/preview.rs`, `src/render.rs`, ni el flujo OpenGL de los STL grandes.

## Bruto

Las seis holguras se introducen en el orden **X−, X+, Y−, Y+, Z−, Z+**,
en las unidades originales del STL (el formato STL no especifica si son mm).
Los extras son `f64`, deben ser finitos y **mayores o iguales a cero**.
El bruto sigue siendo una caja alineada con los ejes del modelo:

- `stock.min[i] = model.min[i] − extra_neg[i]`
- `stock.max[i] = model.max[i] + extra_pos[i]`
- `stock.size[i] = stock.max[i] − stock.min[i]`

Se preserva la edición manual de esquina mínima y dimensiones. Al editar
manualmente, las holguras mostradas se recalculan de los límites reales del STL;
si alguna es negativa significa que el bruto no envuelve por completo el modelo.
El botón **Ajustar a límites del STL** restaura la caja inicial. Si un eje del
STL es plano (extensión cero), hace falta una holgura positiva en al menos una
cara de ese eje; la inicialización actual mantiene espesor 1 unidad por defecto.

## Sistema de coordenadas CNC

Posición: desplazamiento (o preset) respecto a la esquina mínima del bruto.
Orientación: tres ángulos en grados `f64` para X, Y y Z. Se aplica la
convención **R = Rz · Ry · Rx**, con rotaciones activas alrededor de los ejes
fijos del modelo, sistema de mano derecha. La orientación cambia el marco CNC,
**no** la geometría STL ni la posición o las dimensiones del bruto.

- `p_modelo = origen_modelo + R · p_CNC`
- `p_CNC = Rᵀ · (p_modelo − origen_modelo)`

Las transformaciones usan `f64`; solo la proyección de los indicadores
convierte a `f32`. Las pruebas incluyen giros de 90° sobre los tres ejes,
rotaciones combinadas, transformación inversa y rechazo de valores no finitos.

## Límites y validación

El indicador CNC y el bruto son líneas superpuestas en egui sin prueba de
profundidad: las aristas pueden verse delante de las caras STL. No se guarda
un archivo de proyecto al cerrar. No existen trayectorias de mecanizado ni
G-code. El script ejecuta fmt, Clippy, pruebas y compilación Release en Windows,
y restaura los cuatro archivos modificados si alguno falla.
# G19 — Contrato inicial de Contact Solver

**Base:** LaguCAM `main` en `13a439ab15fb5baf8942bcb87af78c6da59d49fb` · **Desarrollo:** rama local `feature/contact-solver-g19` · **Estado:** pendiente de compilar y validar en Windows.

## Responsabilidades

- **Geometry Engine** (`cam_geometry::stl`, `service`): gestiona el STL original, BVH, lectura por bloques y consultas de contacto de esfera sin conocer estrategias CAM.
- **Contact Solver** (`cam_geometry::contact_solver`): recibe una geometría matemática de fresa, posición XY y sobrematerial; resuelve la posición vertical geométrica compatible con esa consulta.
- **Estrategia CAM** (`test_parallel`): decide posiciones y sucesión de pasadas; utiliza el solver con **sobrematerial cero**, manteniendo el resultado del TEST anterior. El nuevo parámetro de sobrematerial no está todavía expuesto en la UI.
- **Biblioteca de herramientas**: mantiene UUID, medidas, metadatos y parámetros tecnológicos; su vinculación al solver queda para una entrega posterior.

## Contrato G19

```text
ContactQuery {
    x_model, y_model,                 // eje de herramienta, marco del STL
    cutter: CutterGeometry,          // geometría matemática; sin UUID
    allowance_model,                 // separación uniforme >= 0
}
    -> ContactSolution { tip_z_model: Option<f64>, contadores de diagnóstico }
    -> ContactSolverError (tipado)
```

Unidades: las **unidades implícitas del STL** para todas las longitudes. No se realiza transformación al marco CNC. Solo se implementa `BallNose`, con eje de herramienta alineado con +Z del STL. `Flat`, `BullNose`, `VBit`, `ConicalFlat` y `ConicalBall` son variantes reconocidas pero **no implementadas**; devolverán error explícito y no simularán geometría esférica.

`tip_z_model = None` indica ausencia de contacto dentro de la zona de influencia, nunca autorización para mover la herramienta. Una respuesta numérica representa el extremo inferior de la fresa esférica a una posición XY determinada: no incluye eje/mango, portaherramientas, mordazas, colisiones, enlaces ni movimientos CNC seguros.

## Sobrematerial uniforme, únicamente para esfera

Sea `r` el radio físico de la esfera y `a >= 0` el sobrematerial. Para la envolvente visible desde +Z se calcula:

```text
Z_punta(r, a) = ContactoEsferico(STL, X, Y, radio = r + a) + a
```

La llamada al motor interno devuelve la altura de la punta de una esfera **de radio `r+a`**. La suma posterior `+a` convierte esa cota en la del extremo inferior de la esfera física de radio `r`.

Esto equivale, en la envolvente superior accesible, a respetar una distancia euclídea uniforme respecto a la superficie triangulada. No se reduce a `Z_nominal + a`: en una rampa de 45° el incremento vertical interior es `a·sqrt(2)`. Con `a=0`, la nueva fachada conserva el resultado anterior de forma bit a bit.

**Restricción importante:** un STL no garantiza un volumen sólido orientado, manifold, ni una cara exterior inequívoca. Esta operación no es una compensación global garantizada de todo sólido cerrado, especialmente en cavidades, socavados, zonas no accesibles o regiones fuera de la envolvente +Z. Para producción se exigirán reglas de topología, orientación y verificación adicional. El sobrematerial se expresa respecto al **modelo nominal final**, no a la operación anterior.

## Alcance de la entrega

- Nueva API tipada, fachada sobre el motor esférico G17 y la misma indexación BVH.
- `test_parallel` consume el nuevo solver con `allowance_model = 0.0`; sin cambio de UX ni del formato `.lagucam`.
- Pruebas unitarias de validación de consulta y geometrías reservadas; pruebas G19 en Windows: regresión exacta G17, plano horizontal, rampa 45°, ausencia de contacto, parámetros inválidos y cancelación.
- **No se implementan** todavía otras formas de fresa, selección UUID, invalidación de trayectorias, generalización a setups rotados, stock remanente, G-code o validación de seguridad CNC.

## Verificación requerida antes de fusionar

```powershell
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo check --locked --all-targets
cargo build --locked --release
```

Además, abrir un STL y generar «Acabado paralelo — TEST» con los mismos parámetros que antes. Si hay error, **no fusionar la rama a `main`**.
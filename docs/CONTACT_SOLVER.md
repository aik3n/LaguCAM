# ContactSolver — contrato y evolución G19–G24

**Revisión técnica:** 2026-11-17 · **Base observada:** `main` en `a10e0c0` · **Evidencia:** revisión estática; no sustituye los tests ni la validación geométrica independiente.

## Contrato y responsabilidades

- **Geometry Engine (`cam_geometry`)**: STL binario protegido, BVH XY, caché de bloques y evaluadores analíticos. Los triángulos originales se decodifican desde `f32` y se operan en `f64`.
- **`ContactSolver`**: API tipada sin UI ni UUID. `ContactQuery` recibe `x_model`, `y_model`, `CutterGeometry` y `allowance_model >= 0`; devuelve `ContactSolution { tip_z_model: Option<f64>, diagnósticos }` o error explícito.
- **Adaptador biblioteca–CAM (`tools/cam_adapter.rs`)**: resuelve UUID global a geometría matemática en unidades del modelo, captura SHA-256 de geometría y comprueba vigencia de previsualizaciones. No inventa diámetros de respaldo.
- **Estrategia (`test_parallel.rs`)**: elige muestras XY, patrón de pasada y sobrematerial; el motor solo resuelve posiciones de contacto. Trayectorias y enlaces se construyen después en módulos separados.

Las consultas actuales trabajan con eje de fresa paralelo a **+Z del modelo físico**, en milímetros. Si hay transformación de entrada, `GeometryAffine` transforma los triángulos originales al construir el BVH y al resolver contactos, **sin reescribir el STL**; la definición de mm/pulgadas, escala, rotación, traslación y pivote reside en `project/model_transform.rs`. La conversión posterior al origen CNC pertenece a F1.2, no al solver. El archivo STL **no declara unidades**, por lo que la selección requiere confirmación del usuario.

## Geometrías implementadas

| Etapa | Geometría | Contacto | Sobrematerial |
| --- | --- | --- | --- |
| G19 | `BallNose` esférica | Sí | Offset euclídeo de envolvente superior; radio de consulta `r+a` y corrección `+a` |
| G21–G22 | `Flat` plana | Sí, disco plano sobre caras/aristas/paredes | Sí; dilatación geométrica con radio de influencia `r+a`, no simple suma vertical |
| G24 | `BullNose` toroidal | Sí, plano central y esquina redondeada | Sí; esquina efectiva `corner+a`, alcance `outer+a`, corrección vertical según perfil |
| Futuro | `VBit`, `ConicalFlat`, `ConicalBall` | **No** | Deben devolver error de geometría no soportada |

En el caso esférico el contacto en rampa de 45° con sobreespesor `a` aumenta aproximadamente `a·sqrt(2)`, no `a`. En plana y toroidal se conserva también la geometría de desplazamiento euclídeo respecto de las superficies evaluadas.

**`tip_z_model = None` significa ausencia de contacto local, no altura de seguridad ni permiso de avance.** El resultado es una cota de herramienta en una posición fija XY. No garantiza ausencia de colisión con mango, pinza, portaherramientas, fijaciones, bruto remanente o durante un segmento entre consultas.

**Verificación específica P1:** comparar contacto y altura obtenidos del BVH con la geometría físicamente transformada y con una referencia analítica independiente, incluyendo mm↔pulgadas, escala no uniforme y rotación; validar rendimiento antes de confiar en resultados de mallas grandes. Las pruebas existentes no acreditan todavía un flujo CNC exportable.

## Precisión, escalabilidad y limitaciones

- G19 conserva la regresión esférica cuando `allowance_model = 0` y G22 conserva el comportamiento plano sin sobreespesor; los módulos contienen pruebas sintéticas y contratos Windows.
- G24 emplea optimización numérica acotada en aristas. Se necesita referencia independiente y control de error máximo, especialmente en tangencias y degeneraciones.
- BVH y caché protegen la memoria contabilizada del motor, **no** garantizan RSS total ni rendimiento medido en el STL real de cerca de 20 M de triángulos.
- STL no garantiza superficie cerrada/manifold ni semántica inequívoca de interior/exterior. La envolvente superior no resuelve cavidades no accesibles ni todo el volumen de mecanizado.
- Los límites y cancelaciones están previstos en el API, pero compilar/testear no verifica trayectorias completas ni resistencia a cualquier STL.

## Pruebas y evidencias

Pruebas de contrato existentes: `tests/g19_contract.rs`, `g21_contract.rs`, `g22_contract.rs`, `g24_contract.rs`, además de unitarias. Los contratos de integración que abren STL están condicionados a Windows. El commit G20–G22 reportó 161 pruebas; esta documentación **no reproduce esa ejecución**. La aprobación de G19 por el usuario corresponde a su ensayo anterior, no acredita por sí sola G24 ni G25.

Para el siguiente hito aplicar el procedimiento de [`TESTING.md`](TESTING.md): `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --all-targets`, `cargo check --locked --all-targets`, `cargo build --locked --release`; añadir referencias analíticas externas y benchmark sobre la malla grande. Ningún resultado autoriza uso CNC sin la verificación de la trayectoria completa.

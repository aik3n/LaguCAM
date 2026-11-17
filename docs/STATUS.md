# Estado verificable de LaguCAM

**Corte documental de auditoría estática:** 2026-11-17. **Código público inspeccionado:** `main` en [`a10e0c0d7c95db773417c2b190e7f4ff9005d8d4`](https://github.com/aik3n/LaguCAM/commit/a10e0c0d7c95db773417c2b190e7f4ff9005d8d4). **Este corte no certifica la ejecución de pruebas ni el contenido del Escritorio.**

| Referencia | Evidencia y alcance |
| --- | --- |
| SHA de código público cubierto por esta **inspección estática** | `a10e0c0d7c95db773417c2b190e7f4ff9005d8d4` |
| Método de revisión | Inspección estática de commits y archivos de GitHub, sin ejecución propia de Rust/GUI/CNC; no se ha inspeccionado la carpeta local del usuario |
| Pruebas anteriores comunicadas | 130 tests Windows en `7de83c9` (históricas); validación funcional G19 comunicada por el usuario tras sus pruebas; 161/161 tests declarados en el mensaje de commit `d12ebcc` (G20–G22), **no reproducidos aquí** |
| Comprobación de tests en `a10e0c0` | **No acreditada en esta revisión**. No hay resultado CI publicado observado; ejecutar en Windows antes de cerrar P1 |
| STL real ~1 GB | Navegación del visor reportada por el usuario en fase anterior; **no** es benchmark del BVH ni del CAM |
| Mecanizado real | **NO APTO PARA PRODUCCIÓN:** todavía no hay post FluidNC, perfil de máquina operativo ni verificación integral de colisiones |

**Nota de continuidad:** este SHA corresponde al código publicado inspeccionado, no garantiza que los archivos del Escritorio coincidan con GitHub. Los chats modifican archivos locales exclusivamente mediante parches `.ps1` ejecutados por el usuario; los commits los realiza él. Cuando se aplique un parche y aún no haya commit, registrar referencia local del parche, **no inventar un nuevo SHA**.

## Capacidades implementadas en el código inspeccionado

| Subsistema | Avance observado | Validación o dependencia pendiente |
| --- | --- | --- |
| **Visor STL** | Importación por bloques, renderer GPU, cámara, selección de pivote, zoom al cursor, cubo de orientación, control ortográfico/perspectiva | Regresión visual Windows y métricas reproducibles con STL cercano a 1 GB |
| **Proyectos y transformación de entrada** | `.lagucam` v2 con `model_transform` opcional, STL fuente intacto, selección mm/pulgadas, escala XYZ, rotación, traslación y pivote; bruto recalculado al aplicar | Integración/round-trip Windows aún no acreditados; comprobar matrices del visor y casos límite; sin backups automáticos de proyecto |
| **Herramientas** | Biblioteca global por UUID, perfiles y validación; geometrías esférica, plana y toroidal enlazadas al ContactSolver mediante adaptador | Otras geometrías reconocidas aún no tienen solver; verificar montaje y colisiones; la huella geométrica no cubre todas las dependencias del cálculo |
| **Contacto geométrico G19–G24** | Solver tipado, BVH que indexa triángulos físicamente transformados cuando procede, sobrematerial esférico/plano/toroidal; límites, diagnósticos y cancelación | Pruebas analíticas independientes y rendimiento sobre malla ~20 M; comprobar equivalencia del visor y CAM tras transformación |
| **Acabado paralelo experimental** | Pasadas X/Y, área, sobreespesor, patrón unidireccional/zigzag, elección de fresa, previews y cancelación | Error entre muestras, barrido continuo, colisiones y material remanente; aún experimental |
| **Trayectorias G25** | Segmentos neutrales de corte, enlaces/descensos/retiradas propuestos, marcadores de inicio/fin y validación **estructural** con incidencias por movimiento | No comprueba volumen barrido ni obstáculos reales; no autoriza ejecutar G-code |
| **Contrato CAM F1.1 y procedencia P1-A** | Vista inmutable con F/S opcionales y `CamProvenance` para invalidar previews por herramienta, STL, bruto, transformación o parámetros geométricos | El constructor de la vista no exige informe estructural apto; F/S no autorizan máquina; identidad STL por muestras FNV-64 no es hash criptográfico completo |
| **Coordenadas F1.2** | Conversión modelo → coordenadas de trabajo en mm con traslación y giro alrededor de Z; rechaza inclinaciones Rx/Ry | Cierre de unidades y marco de máquina, integración con post y validación de setups reales |
| **Máquinas y postprocesado** | FluidNC como primer objetivo de diseño | **No implementados** perfil operativo de máquina, post, simulación/verificación completa ni exportación de programa |

## Auditoría de P1 respecto al corte previo

- **`4a51d65`:** `CamProvenance` para invalidación cuando cambian entradas relevantes; separa identidad geométrica de avances/RPM, sin traza exportable todavía.
- **`94b6d8c`:** `ModelTransform` y `GeometryAffine`; unidades, pivote, escala, rotación y traslación sin modificar el STL; se conectan renderer, BVH, operaciones y persistencia.
- **`a10e0c0`:** ajustes de advertencias/caché en tests y tipos reservados para post; **no añade G-code**.
- **Hallazgos a resolver antes de cerrar P1:** `main.rs` usa identidad de matriz visual como alternativa silenciosa si falla `preview_matrix()`; el editor desecha resultados geométricos incluso al cambiar nombre o F/S; falta evidencia de prueba extremo a extremo de importación/unidades/transformación/origen/trayectoria/round-trip.
- **Calificación de evidencia:** hallazgos de inspección estática; **no** regresiones reproducidas en Windows. `src/test_parallel.rs` acumula UI/hilos/resultados; solo refactorizar con alcance y pruebas concretas, sin reestructuración general.

**Gobierno de chats y sucesión:** `CHAT_ROLES.md`, `WORKFLOW.md`, `HANDOFF.md`; no forman parte del estado de validación geométrica.

## Seguridad: límites que no deben omitirse

- `None` en ContactSolver significa falta de contacto, no permiso de desplazamiento.
- `is_structurally_valid()` solo significa continuidad, orden y cotas coherentes según el contrato G25; **no** detecta colisiones con mango, portaherramientas, utillaje, stock remanente o máquina.
- El estado UI «Ready» corresponde al cálculo experimental con movimientos de corte y validación estructural, no a una operación lista para producción.
- STL no almacena unidades: el modelo permite configurar **mm o pulgadas de origen** y transformar a milímetros físicos; no inferir la unidad real del STL ni exportar sin verificarla. Si falta información, la opción inicial es mm, no un dato certificado por el archivo.
- El frame F1.2 conserva el eje vertical de tres ejes; el control CNC no hereda automáticamente una rotación `Rz` por seleccionar `G54`.

## Próximo objetivo: prototipo 3 ejes de extremo a extremo

1. **Cerrar integración y vigencia:** origen, unidades, biblioteca, F/S, configuración de operación y dependencias de la trayectoria; sin usar datos obsoletos.
2. **Validador adicional independiente:** referencia de contacto, tolerancia entre puntos, límites, volumen barrido, colisiones de corte/mango/utillaje, alturas y secuencias. La validación estructural existente **no basta**.
3. **Perfil mínimo de máquina y post FluidNC:** separar estrategia → trayectoria neutral → validación → post; contrastar capacidad concreta de FluidNC, homing, límites y cambio manual.
4. **Flujo UI completo:** crear/abrir proyecto, seleccionar herramienta y setup, generar, visualizar incidencias y exportar **solo tras verificaciones definidas**.
5. **Validación Windows:** `fmt`, `clippy`, `test`, `check`, `build --release`, simulación offline independiente, prueba en vacío y benchmark de STL grande antes de considerar corte real.

No priorizar STEP, cinco ejes, desbaste u otras estrategias hasta cerrar un primer ciclo reproducible de acabado paralelo.

**Documentos:** [`ROADMAP.md`](ROADMAP.md) (prioridades), [`WORKFLOW.md`](WORKFLOW.md) (parches `.ps1`), [`TESTING.md`](TESTING.md) (evidencia), [`DECISIONS.md`](DECISIONS.md) (decisiones).

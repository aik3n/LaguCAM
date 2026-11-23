# Política de pruebas, regresión y seguridad CNC

<!-- LAGUCAM-MASTER-DOC-CUT: 6d496200f5bf6ec70bb0bb8249543b315d3e1854 -->

**Politica vigente desde:** 2026-11-17. **Extension de cobertura:** 2026-11-23 (`6d496200`). Ejecuciones concretas y SHA publico en [`STATUS.md`](STATUS.md). Esta política se aplica a **parches `.ps1` locales** ejecutados por el usuario. Actualizar Markdown no ejecuta Rust, GUI ni CNC.

## Comprobaciones de identidad UUID y proyecto v2

- `ToolId` UUID textual compartido en biblioteca, operación y `.lagucam` v2; probar valor válido, inválido, numérico antiguo y JSON nulo.
- El esquema `.lagucam` v1 se rechaza expresamente: **es aceptable durante desarrollo**; no migrar por compatibilidad ni borrar archivos personales.
- Verificar que `toolpath_id` sigue siendo independiente y que guardar/abrir proyectos sin operaciones o con referencias UUID no altera datos del modelo ni del bruto.

## Tests SHA-256 geométrico (función implementada; verificar por commit)

- Idéntico tipo y geometría → mismo hash, aunque difieran UUID, revisión, nombre o metadatos.
- Ø6 → Ø4 → Ø6: primera y tercera huella iguales si todas las cotas relevantes coinciden; la de Ø4 diferente.
- Variar cada parámetro físico relevante en las seis familias: cambia hash. Cambiar RPM, avance, nombre o notas: no cambia hash geométrico.
- Seis vectores SHA-256 de referencia independientes para las seis variantes (incluida toroidal); representación canónica binaria y versionada en milímetros: orden de campos, codificación f64, `+0.0`/`-0.0`, NaN/infinito y límites numéricos.
- El código del hash se mantiene independiente de las operaciones y `.lagucam`. Antes de validar una futura trayectoria habrá que contrastar UUID, hash y el resto de sus dependencias.


**Estado:** criterios para el prototipo en curso; **no** constituyen certificación del software.

## Validación de parches y separación de evidencia

Para **cada `.ps1`** verificar: precomprobación sin escribir (`-CheckOnly`), hashes de todos los archivos, rechazo de conflicto, ausencia de rutas externas y operaciones Git, staging en el mismo volumen, reejecución sin duplicados, notificación de errores y recuperación cuando falla un reemplazo. Antes de entregar, el chat debe ensayar en una **copia temporal** cuando haya entorno compatible; sin PowerShell/Windows accesible, marcar esa ejecución como **pendiente**. No aceptar un mensaje «aplicado» como prueba de compilación, ni un `cargo test` como prueba de GUI o de seguridad CNC.

El usuario aporta salida de aplicación y de pruebas. **Un parche documental no requiere compilar Rust**. Los temporales de recuperación de código/documentación son transitorios; D-039 prohíbe los backups automáticos de proyectos `.lagucam`, no la recuperación controlada de una modificación del código. Ver [`WORKFLOW.md`](WORKFLOW.md) y roles de revisión en [`CHAT_ROLES.md`](CHAT_ROLES.md).

## 1. Comandos de calidad de Rust

Comandos de referencia desde la raíz del repositorio:

```powershell
cd "$env:USERPROFILE\Desktop\LaguCAM"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo check --locked --all-targets
cargo build --locked --release
```

Registrar sistema, versión Rust, salida real y si alguno no pudo ejecutarse. La existencia de `Cargo.lock` fija el grafo de dependencias; no equivale a verificar licencias o seguridad de todas ellas. El árbol de `main` inspeccionado en `5cd41c7` no contenía workflow de CI activo; **no afirmar que CI corre** hasta incorporarlo y observar sus resultados.

## 2. Pruebas del visor y proyecto

### Regresión de operaciones e iconografía (`6d496200`)

- Alternar `visible` con ojo Lucide en operaciones activas e inactivas; comprobar que solo cambia render 3D, no cálculo, NC, caché o estado `active`.
- Alternar `active` conservando la visibilidad; cálculo y generación conjunta deben incluir exclusivamente activas, en orden.
- Guardar `.lagucam`, cerrar y abrir: recuperar ambas propiedades; proyecto previo sin `visible` debe abrir con trayectoria visible. La prueba automatizada de persistencia no sustituye al ensayo manual.
- Arrastrar desde el icono de operación, no desde el nombre: comprobar selección, doble clic, reordenación, identidad del resultado CAM, marcadores y ausencia de arrastres accidentales.
- Abrir/cerrar y desplegar repetidamente el árbol; los recursos Lucide no deben provocar bloqueo por reentrada de `egui::Context::data_mut`; comprobar los dos temas.
- Comprobar que el editor carece de botones de generación duplicados y notas auxiliares retiradas; `Vista` ya no muestra diagnóstico experimental ni vista superior duplicada. Comprobar que `Centrar vista` y `Restablecer paneles` siguen disponibles.
- Ejecutar sobre el árbol final `cargo fmt --all -- --check`, `cargo test --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`; registrar salida y SHA local. Las **382 pruebas reportadas** en el hito no acreditan automáticamente Clippy final del commit.


- STL binario pequeño conocido, truncado, conteo erróneo, archivo vacío, coordenadas no finitas y carga cancelada.
- Abrir un STL válido manteniendo el anterior ante fallo de importación o GPU.
- Probar giro, zoom, desplazamiento, centrado, profundidad y cambios de tamaño de ventana.
- Validar bruto con dimensiones positivas y límites finitos; extras independientes y avisos si no rodea al modelo.
- Probar anclas de origen, offsets, cambios de referencia y transformaciones inversas de rotación.
- Al añadir persistencia: round-trip exacto de parámetros `f64` serializados, compatibilidad de versiones, rutas relativas, STL perdido/modificado, guardado interrumpido y protección frente a cambios sin guardar.

Comprobar también el selector de temas: cambio real de tema Claro/Oscuro sin reiniciar, restauración desde `settings.json` y ausencia de alteraciones del visor 3D.

## 3. Benchmark de STL grande

La referencia del usuario es un STL real de **casi 1 GB** que se abre y mueve fluidamente. Cada cambio que afecte importación, render, memoria, cámara o UI debe compararse con esa referencia cuando sea posible.

Registrar como mínimo:

| Magnitud | Dato |
| --- | --- |
| Archivo | Tamaño exacto, número de triángulos, origen del archivo si se puede compartir sin datos sensibles |
| Equipo | SO, CPU, RAM, GPU, VRAM, driver |
| Carga | Tiempo de apertura hasta primera imagen y hasta vista completa |
| Memoria | Pico RAM, estimación/medición de VRAM, errores GPU |
| Fluidez | FPS o latencia de frame mientras gira y hace zoom, idealmente p95 |
| Robustez | Cancelación, fichero inválido, carga sucesiva y liberación de recursos |
| Resultado | PASA/NO PASA, notas de reproducibilidad |

No inventar números cuando faltan. No subir a Git el STL grande salvo permiso y una política explícita de almacenamiento.

## 4. Pruebas geométricas CAM: obligatorias para validar el prototipo

El contacto esférico y el muestreo TEST ya están codificados, **pero aún no constituyen trayectorias verificadas**. Antes de considerar una estrategia apta para producir G-code:

- Superficies patrón: plano horizontal/inclinado, esfera, cilindro, escalón, pared vertical y STL no cerrado.
- Herramientas patrón con fórmulas de contacto conocidas; plano, esférica y cónica según se añadan.
- Comprobación de cota de error geométrico y de casos degenerados/numéricos.
- Comparación contra solución de referencia independiente o cálculo analítico; no validar un algoritmo únicamente con su propia salida.
- Comprobar colisión de parte cortante, mango/cuello, portaherramientas, bruto, material remanente y utillaje cuando cada modelo esté disponible.
- Trayectorias sin saltos no explicados, sin penetraciones injustificadas ni movimientos de enlace inseguros; verificar su relación con el setup.
- Ensayar escalabilidad con muchas caras sin reducir tolerancia en silencio.

## 5. Seguridad de máquina y postprocesado

Hasta completar pruebas específicas, cualquier G-code emitido será **experimental y no validado para ejecución directa**.

La cadena de verificación debe incluir:

1. Contrastar unidades, sistema de trabajo, ejes, sentido Z y modelo de máquina.
2. Verificar homing, soft limits, volumen de trabajo, alturas de retirada y zonas de seguridad reales.
3. Verificar herramienta, diámetros, longitud libre, sujeción y límites de husillo/avance.
4. Interpretar el G-code con el **dialecto y configuración reales de FluidNC**; revisar modos modales, arranque/parada, `G53`/`G54`, retracts y fin.
5. Definir y validar la parada/cambio manual y la referencia de longitud de herramienta. No dar por válida una línea `M6` genérica.
6. Simular/comprobar offline con un medio independiente y revisión humana.
7. Realizar prueba sin herramienta/por encima del material o en vacío, con condiciones seguras y parada de emergencia disponible, antes de intentar un corte real.

**Una simulación no prueba por sí sola que la máquina no pueda chocar.** Seguridad y responsabilidad sobre la máquina real requieren verificación en el equipo físico.

## 6. Qué cuenta como validación

- Prueba unitaria: evidencia de una propiedad concreta de una función.
- Build: prueba de compilación, **no** de comportamiento visual ni de geometría correcta.
- Prueba gráfica: valida interacción y representación en el hardware/driver ensayados.
- Benchmark: cuantifica rendimiento de una versión y configuración.
- Validación CAM: compara trayectoria, error y restricciones contra referencias independientes.
- Prueba de máquina: requiere procedimiento controlado; nunca derivarla de una revisión de código.

Registrar explícitamente cuál se realizó y cuál falta.

## 7. Pruebas por parche y contratos G19–G25 / F1

1. Guardar `parche_*.ps1` en el Escritorio, al lado de `LaguCAM`. Ejecutar la instrucción exacta facilitada; **no hay operaciones Git del chat**.
2. Antes de aplicar, el script debe comprobar estructura y base local, identificar conflictos y no sobrescribir cambios inesperados. Informar archivos afectados y abortar ante error.
3. Aplicar el parche y comprobar el comportamiento. Si hay código Rust, usar los comandos de la sección 1; si solo hay documentación, comprobar contenido y enlaces sin recompilar por obligación.
4. Para G19–G24: pruebas de contacto de esférica, plana y toroidal con geometría independiente, sobreespesor, tangencias, paredes y regresiones bit a bit de casos nominales.
5. Para G25: verificar continuidad, orden, alturas, patrones, huecos de contacto, segmentos aislados y que **el validador estructural no se anuncie como comprobador de colisiones**.
6. Para F1.1–F1.2: comprobar F/S válidos/faltantes, unidades, cero de trabajo, Rz, rechazo de Rx/Ry, conservación de movimientos originales y errores de desbordamiento.
7. **Cierre del prototipo:** añadir comprobaciones de volumen barrido/cuerpos/stock/utillaje, post FluidNC con casos golden, simulación independiente, pruebas sin material y rendimiento del STL grande. Las restricciones de una CNC real deben comprobarse antes de uso.

Registrar resultado **reportado por el usuario** y entorno/versión concreta. Nunca afirmar un número de tests sin la salida correspondiente.

## 7.b. Contratos nuevos a probar antes de cerrar lote, NC y preparacion de maquina

**No son pruebas ejecutadas en `85dfda2`; son criterios de aceptacion propuestos/pendientes.**

1. **Inicio de estrategia D-045:** el primer punto neutral coincide exactamente con el primer XY de mecanizado y Z de clearance sobre el bruto; ningun XY externo de aparcamiento al inicio; los enlaces internos entre pasadas permanecen a despeje y sin saltos.
2. **G53 D-046 (NO implementado):** validar cuando se implemente retirada unica Z, estado de homing y MPos conocida, limite de ejes y posterior posicionamiento XY al inicio; rechazar coordenada maquina no disponible, Z no segura y Start no interpretable. La posicion CAM teorica no confirma la posicion fisica.
3. **Cola conjunta en `85dfda2`:** cero operaciones activas; seleccion parcial; cache reutilizada; una fallida con resto exitoso; cancelacion durante calculo; reordenacion durante el trabajo; desactivacion en curso; cambio de proyecto; error al leer STL; ausencia de NC conjunto.
4. **No obsolescencia:** cambio STL/model_transform, bruto, origen, herramienta UUID/hash, parametros de estrategia o texto NC invalida solo lo dependiente; cambios esteticos no recalculan CAM.
5. **NC experimental:** mismo G-code final para preview, informe y reproducción; metricas G0/G1 calculadas con modos finales, tiempo G1 excluye aceleraciones, G0 y auxiliares; Start personalizado no se convierte en seguridad ni se ignora.
6. **Verificador y simulacion:** probar fondos/alturas, G0 potencialmente en material, entrada G1 profunda, seleccion de fresa, movimientos de Start/End, desbaste de stock original vs material restante, limitaciones 2,5D y sin portaherramientas.
7. **Render y GUI:** controles compactos, vista aislada, abrir/cerrar simulacion, identidad de herramienta, aviso enlazado a movimiento y FPS/RAM sin extrapolar del visor STL.

**Comandos de calidad en Windows, registrar SHA y log real:** `cargo fmt --all -- --check`; `cargo check --locked --all-targets`; `cargo test --locked --all-targets`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo build --locked --release`. Las pruebas de maquina son otro hito, con procedimiento controlado y responsabilidad humana.

## 8. Trazabilidad de pruebas por hito

La fase **validación** precede a la fase **documentación**. Desarrollo entrega SHA, versión Rust, SO, comandos y resultado; documentación registra lo verificado y lo comunicado, sin repetir pruebas imaginarias.

En la integración `7de83c9` se **comunicaron** 130 pruebas unitarias correctas en Windows, `cargo fmt`, `cargo check` y compilación Release; también se comunicaron pruebas de interfaz. Son informes de ejecución, **no una nueva ejecución de este chat** ni evidencia de precisión geométrica a escala real. `13a439a` es un commit posterior; las pruebas se atribuyen a su SHA real.

Para el motor CAM, añadir benchmarks reproducibles con un STL patrón y uno grande: tamaño, triángulos, máquina, radio de fresa, cantidad de consultas, tiempos de índice/consultas, RAM máxima observada, error geométrico frente a referencia y regresión. Cuando falten resultados indicar **no medido**, no extrapolar del visor.

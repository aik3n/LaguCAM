# Política de pruebas, regresión y seguridad CNC

**Estado:** criterios para futuras etapas; no constituye certificación del software. **Fecha:** 2026-11-14.

## 1. Comandos de calidad de Rust

Comandos de referencia desde la raíz del repositorio:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

Registrar sistema, versión Rust, salida real y si alguno no pudo ejecutarse. La existencia de `Cargo.lock` fija el grafo de dependencias; no equivale a verificar licencias o seguridad de todas ellas. El árbol de `main` examinado no contiene un workflow de CI; **no afirmar que CI corre** hasta incorporarlo y observar sus resultados.

## 2. Pruebas del visor y proyecto

- STL binario pequeño conocido, truncado, conteo erróneo, archivo vacío, coordenadas no finitas y carga cancelada.
- Abrir un STL válido manteniendo el anterior ante fallo de importación o GPU.
- Probar giro, zoom, desplazamiento, centrado, profundidad y cambios de tamaño de ventana.
- Validar bruto con dimensiones positivas y límites finitos; extras independientes y avisos si no rodea al modelo.
- Probar anclas de origen, offsets, cambios de referencia y transformaciones inversas de rotación.
- Al añadir persistencia: round-trip exacto de parámetros `f64` serializados, compatibilidad de versiones, rutas relativas, STL perdido/modificado, guardado interrumpido y protección frente a cambios sin guardar.

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

## 4. Tests geométricos CAM futuros

Antes de enviar una estrategia a producción:

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

## Ensayos de pivote de navegación (010)

1. Abrir STL pequeño; doble clic en una cara: debe aparecer una cruz discreta en el centro de giro. Girar y hacer zoom sin que el punto se desplace con respecto al modelo.
2. Doble clic en el cielo: se mantiene el pivote y aparece mensaje, sin salto de cámara.
3. `Vista > Centrar vista`: restaura centro visual de bounds y encuadre inicial; desaparece la marca de pivote personalizado.
4. Girar, hacer zoom y desplazar; comprobar bruto, marcadores de origen CNC y proyectos `.lagucam` previos.
5. Repetir sobre STL cercano a 1 GB: comprobar que no se vuelve a leer desde disco, que no aumenta memoria de forma proporcional al STL y que el clic no bloquea perceptiblemente el visor.
6. Con GPU y driver de Windows, comprobar profundidad con MSAA 4×, vistas en perspectiva, bordes de triángulo, paneles redimensionados y escalado DPI distinto de 100%.

La lectura GPU es visual, no una consulta geométrica exacta para CAM.

## Ensayos 011 — Clic simple sin movimiento inmediato

1. Sobre una zona reconocible del STL, hacer clic simple. Confirmar que ni el modelo ni el encuadre saltan en ese instante, y que la marca aparece sobre el punto elegido.
2. Arrastrar con el boton izquierdo: la escena rota alrededor de ese punto manteniendolo fijo en pantalla. Probar otros puntos, pendientes fuertes y modelos planos.
3. Clic sobre el cielo: conserva el pivote anterior. «Vista > Centrar vista» recupera pivote y encuadre iniciales.
4. En Origen CNC, seleccionar puntos rojos sin cambiar el pivote de navegacion. Verificar pan derecho, zoom y bruto.
5. Repetir en el STL real cercano a 1 GB, comprobando fluidez, ausencia de otra lectura del archivo y funcionamiento de biblioteca y `.lagucam`.
6. Verificar en Windows con DPI distinto de 100% y MSAA 4x.

## Ensayos 012 — Pulsacion y arrastre continuos

1. En el STL, pulsar boton izquierdo sin soltar: se selecciona y marca un pivote sin mover la vista.
2. Manteniendo pulsado, arrastrar inmediatamente: la orbita empieza alrededor de ese mismo punto. Probar movimientos rapidos para comprobar que no hay salto ni desplazamiento inicial alrededor del pivote antiguo.
3. Soltar: el marcador persiste; volver a girar y comprobar que el ultimo pivote se conserva. «Vista > Centrar vista» lo restablece.
4. Pulsar en el cielo: no sustituye el pivote. Arrastrar desde el cielo puede girar con el pivote anterior.
5. Verificar puntos rojos de Origen CNC, bruto, zoom y pan; repetir con DPI alto, MSAA y STL real de gran volumen.
6. Comprobar tests de `PivotPress` y de `navigation.rs`, Clippy y compilacion release.

## Ensayos 013 — Indicador solo mientras se rota

1. Pulsar en la superficie y mantener: aparece la marca amarilla en el punto, sin salto de camara. Arrastrar sin soltar: rota alrededor de ese pivote.
2. Soltar sobre el modelo y tambien tras salir del viewport: la marca desaparece. Girar de nuevo debe seguir seleccionando de inmediato el nuevo pivote.
3. Clic breve: establece pivote aunque no llegue a verse la marca al soltarse inmediatamente. Girar desde el fondo debe conservar el pivote anterior; si se pulsa para rotar, la marca solo aparece durante el gesto.
4. Probar boton derecho/central para desplazar, rueda para zoom, prioridad de puntos rojos del origen CNC y «Vista > Centrar vista».
5. Revisar pruebas Rust del estado del gesto y botones por defecto; Cargo fmt, clippy, test, release, rendimiento con STL grande, DPI y MSAA.

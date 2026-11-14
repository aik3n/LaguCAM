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

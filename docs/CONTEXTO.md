# LaguCAM — contexto vigente para continuar el desarrollo

**Base inspeccionada:** [`26f794e1df10f4703dcdd13204d430c397f69979`](https://github.com/aik3n/LaguCAM/commit/26f794e1df10f4703dcdd13204d430c397f69979). Este documento resume una **inspección estática del repositorio**, no una ejecución de Windows. Al reanudar, verificar el HEAD y la copia local; esta referencia no pretende describir commits futuros.

## 1. Qué queremos construir

LaguCAM es una aplicación **CAM libre (licencia MIT)** para preparar mecanizados CNC. El primer producto útil es un flujo de **fresado de 3 ejes sobre modelos STL grandes**, empezando por el **acabado paralelo**, con herramientas de biblioteca, trayectorias, G-code y una vista orientativa del resultado. El caso fundador incluye relieves/topografía en madera, pero LaguCAM **no debe limitarse a madera ni topografía**.

Se priorizan **simplicidad de uso, rendimiento, robustez, mantenimiento y portabilidad**. Otras estrategias, formatos como STEP y 4/5 ejes son posibilidades futuras, **no objetivos actuales**. No hay gestión de fijaciones en la primera etapa.

## 2. Flujo funcional y arquitectura

```text
STL externo -> proyecto (.lagucam) -> bruto / origen de trabajo
             -> biblioteca de fresas -> operaciones CAM activas
             -> estrategias / trayectorias neutrales
             -> planificador (orden y enlaces entre operaciones)
             -> postprocesador -> G-code NC definitivo
             -> interprete NC -> verificacion parcial + simulacion orientativa
             -> exportacion NC
```

La aplicación usa **Rust 2024**, `egui/eframe`, `glow/OpenGL` y configuración portable. El paquete Cargo se llama `cam-libre-visor`, aunque la aplicación se denomina LaguCAM.

| Subsistema | Fuente principal | Responsabilidad |
| --- | --- | --- |
| STL y visor | `stl.rs`, `preview.rs`, `render.rs`, `navigation.rs` | Cargar y navegar mallas grandes sin convertir la aproximación gráfica en geometría CAM |
| Proyecto y coordenadas | `project/*`, `project/persistence.rs` | Bruto, origen, operaciones, transformación STL no destructiva, persistencia `.lagucam` |
| Herramientas | `tools/*`, `tools_ui.rs` | Biblioteca global UUID y huella geométrica; selección por operación |
| Geometría CAM | `cam_geometry/*` | ContactSolver sobre geometría original y consultas espaciales |
| Estrategia | `test_parallel.rs`, `toolpath*.rs` | Acabado paralelo y trayectorias neutrales |
| Planificación y NC | `cam_plan.rs`, `nc_post.rs`, `cam_combined_nc.rs`, `nc_export_ui.rs` | Enlaces entre operaciones, NC individual/conjunto, post y exportación |
| Interpretación y verificación | `nc_interpreter.rs`, `nc_verify.rs` | Reconstrucción del NC final y comprobaciones parciales |
| Simulación | `sparse_material.rs`, `sparse_runtime.rs`, `material_ui.rs`, `nc_playback_ui.rs`, `sparse_scene_gpu.rs` | Heightfield de material restante, cálculo final o reproducción, visualización por bloques |
| Configuración de máquina | `machine_ui.rs`, `post_ui.rs`, `portable.rs` | Datos de máquina y post independientes del proyecto |

## 3. Decisiones de producto que deben respetarse

- **Una operación / varias operaciones:** pueden calcularse individualmente o en conjunto según la selección. La **activación** de una operación determina su participación en el cálculo/NC y es independiente de la **visibilidad** de su trayectoria.
- **Operación CAM:** cada estrategia define su primer XY y su propia Z de seguridad; las aproximaciones, retiradas y enlaces internos son su responsabilidad.
- **Entre operaciones:** el planificador produce transiciones neutrales y solicita herramientas; no se modifican trayectorias de corte para resolver enlaces. El post no inventa enlaces geométricos.
- **NC individual y conjunto:** deben compartir motor interno sin imponer al individual restricciones del conjunto. Mantener las dos interfaces públicas, sus reglas de Start/End, despejes, T/M6 y la precedencia de errores.
- **Exportación NC:** bloquear errores reales, no exigir casillas para aceptar riesgos; regenerar antes de guardar. El operador es responsable de máquina, origen, herramientas y fijaciones.
- **Persistencia:** proyecto `.lagucam` con referencia externa al STL; no copiar gigabytes de malla en el proyecto ni crear backups automáticos de proyectos.
- **Bibliotecas:** fresa global con UUID y SHA-256 de geometría; configuración portable y separada de máquina y postprocesador.
- **Seguridad:** una trayectoria o un G-code que pasa pruebas estructurales **no demuestra** ausencia de colisiones ni autoriza mecanizado real.

Los límites exactos entre estrategia, planificador, post, intérprete, simulador, verificador, UI y operador están en [`RESPONSABILIDADES_CAM_NC.md`](RESPONSABILIDADES_CAM_NC.md).

## 4. Simulación: decisión vigente

**Un único motor principal: Heightfield 2,5D disperso con interpolación entre alturas vecinas y renderizado por bloques.** Se descarta el prototipo Tri-Dexel como motor activo.

La UI compacta del commit inspeccionado tiene dos modos:

- **Resultado final (principal):** aplica toda la trayectoria interpretada y presenta el material restante de una vez, sin historial de rebobinado.
- **Reproducción progresiva:** usa el mismo Heightfield, con trabajador asíncrono, herramienta visual, controles de reproducción/pausa/reinicio y velocidad.

La eliminación se calcula a partir del **NC interpretado**, no del dibujo de la trayectoria. El material se representa mediante una altura por punto XY, con tiles de 128 × 128 muestras; la GPU actualiza regiones modificadas. La resolución XY es configurable (valor por defecto observado: **0,05 mm**); **no implica una garantía metrológica de 0,05 mm** ni escalabilidad ilimitada. El visor tiene límites de rejilla/memoria que deben medirse con piezas reales.

**Limitaciones aceptadas:** vista orientativa de fresado vertical 3 ejes; no representa socavados, superficies con múltiples cotas Z por XY, colisiones, soportes, portaherramientas ni efectos reales de máquina. El cálculo de material en la implementación actual admite geometrías **plana y esférica**; el ContactSolver CAM soporta además toroidal. No confundir ambos alcances.

No crear otro motor volumétrico por inercia. **Validar y mejorar la solución existente primero.**

## 5. Qué hay y qué falta comprobar

**Observado en el código del commit:** visor STL, transformación no destructiva, bruto, origen, operaciones y biblioteca, acabado paralelo experimental, planificación/NC individual y conjunto, post configurable, intérprete NC, verificación parcial y las dos modalidades de simulación Heightfield.

**No acreditado por esta revisión:** compilación y pruebas completas de este SHA en Windows, calidad visual final con casos reales grandes, rendimiento CPU/GPU y RAM a distintas resoluciones, ausencia de regresiones en la GUI, colisiones físicas reales, compatibilidad completa con FluidNC o seguridad de máquina. La validación de un subsistema no certifica el resto.

Se ha observado anteriormente que `cargo test --locked --all-targets` puede fallar en ejemplos auxiliares de diagnóstico por imports `crate::nc_interpreter` propios de los ejemplos. Comprobar si sigue ocurriendo antes de atribuir un fallo al motor CAM; **no omitir silenciosamente los errores**.

## 6. Próxima prioridad de trabajo

1. **Comprobar en Windows** las dos vistas Heightfield con el mismo NC: que el resultado final sea consistente con el final de la reproducción.
2. Medir en STL reales grandes **tiempo de cálculo, memoria y FPS**, separados por CPU del cálculo de material y GPU/visor. Anotar tamaño de bruto, paso XY, herramienta y número de movimientos.
3. Corregir primero defectos reproducibles (saltos de cursor, actualizaciones de tiles, comportamiento al cancelar o cambiar operación), preservando los contratos y sin rediseñar por defecto la tecnología.
4. Mantener independientes la simulación orientativa y la validación técnica del NC. No presentar el resultado visual como comprobación de seguridad.

**Regla de documentación:** actualizar este archivo solo cuando cambien el objetivo, una decisión vigente, las capacidades efectivamente comprobadas o la prioridad inmediata. No incluir cronologías, parches sucesivos, cifras de tests antiguas, listas de SHA anteriores ni registros de conversaciones.

## 7. Cómo trabajar desde otro ordenador o chat

La copia local es `$HOME\Desktop\LaguCAM` en PowerShell 7. No usar rutas que incluyan un nombre de usuario concreto. El usuario ejecuta parches autocontenidos `.ps1` con `-CheckOnly`, validación y reversión, y gestiona Git. Las copias de rescate de parches deben permanecer **fuera del repositorio** para evitar versionarlas por accidente.

**Primer paso del chat nuevo:** leer `README.md`, `AGENTS.md`, este `CONTEXTO.md` y `RESPONSABILIDADES_CAM_NC.md`; verificar el SHA actual y abrir los archivos de código afectados antes de afirmar qué está implementado o preparar un parche.

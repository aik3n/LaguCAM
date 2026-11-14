# Estado actual de LaguCAM

## Corrección puntual 021 — tema activo (pendiente de prueba en Windows)
- `src/options.rs` fija explícitamente `egui::Theme` al aplicar la preferencia y al restaurarla desde la configuración portable, además de aplicar la paleta de colores correspondiente.
- No se cambia el formato `settings.json`; los ajustes de hash geométrico 020 que existan en el árbol local se conservan intactos.
- Las pruebas Cargo y de interfaz de esta corrección no han sido ejecutadas en este entorno.
> **Actualización preparada 020 (base `db5ee13bf70a6cdb861a5e94b279413606bd7b19`, sin commit):** implementada función independiente de huella geométrica SHA-256 v1 en `src/tools/hash.rs`, con pruebas automáticas. Pendientes: ejecución real de Cargo y regresión del visor STL grande por el usuario. No se conecta a operaciones ni invalida trayectorias.

**SHA de código revisado:** [`f1e436b976eb6f4f88482b00ae5aeff2761e08ee`](https://github.com/aik3n/LaguCAM/commit/f1e436b976eb6f4f88482b00ae5aeff2761e08ee) · `main` · 14-11-2026. **Método:** lectura estática de código y documentación GitHub; los ensayos del usuario se citan como tales. No se ha compilado ni ejecutado el programa en esta revisión.

## Estado de capacidades

| Área | Estado | Evidencia y alcance |
| --- | --- | --- |
| STL binario grande y visor OpenGL | **Implementado; caso real validado por el usuario** | Importación por bloques, navegación de un STL de casi 1 GB en su equipo |
| Cámara, zoom/pan y pivote por profundidad | **Código observado** | Picking visual basado en GPU, no consulta geométrica exacta CAM |
| Árbol, bruto y origen CNC | **Código observado y pruebas de usuario de versiones previas** | Extras del bruto, representación, offsets, selección de anclas y orientación matemática |
| Guardar/abrir proyecto | **Implementado en código** | `.lagucam` v2, referencia externa STL, unidades mm, guardado protegido |
| Biblioteca global de fresas | **Implementada en código** | Cinco tipos, editor, filtro/ordenación y perfil 2D, `config/tools.json` |
| Identificadores de herramientas | **Implementado en código** | `ToolId` UUID textual compartido entre biblioteca, operaciones y `.lagucam` desde `f1e436b` |
| Preferencias, paneles y apariencia | **Código observado** | Tema claro/oscuro, docking visual, configuración portable |
| Selección de fresa por operación | **Pendiente** | No se resuelve todavía el UUID desde una operación |
| Huella geométrica SHA-256 | **Código preparado en 020; pruebas Cargo pendientes** | Función binaria v1 y pruebas unitarias; no conectada a CAM |
| Estrategias CAM y trayectorias | **No implementado** | Operaciones siguen siendo marcadores |
| Máquina y postprocesador FluidNC | **No implementado** | Arquitectura acordada, sin salida G-code |
| Colisiones, simulación, STEP/B-Rep, 4/5 ejes | **Futuro** | No hay capacidad operativa |

## Estado del modelo de herramienta

La biblioteca es la **única fuente editable** de definición de fresa; los proyectos almacenarán solo UUID, no copias ocultas. `src/tools/model.rs` contiene las cinco geometrías en `f64`, validación y un perfil discretizado para **dibujo**, no para calcular contacto de mecanizado. Cada herramienta tiene UUID persistente y revisión creciente; las operaciones y persistencia comparten el mismo tipo identificador.

Para evitar trayectorias calculadas con definiciones distintas, se acordó crear una huella **SHA-256 de geometría** (tipo, diámetros, radios, ángulos, longitud de corte, diámetro del mango y longitud total, más dimensiones físicas relevantes cuando existan). No incluirá UUID, revisión, nombre, notas, RPM ni avance. Una trayectoria futura registrará UUID, huella geométrica y restantes dependencias del cálculo; no se podrá exportar si no coincide la definición efectiva de la que depende. Ø6 → Ø4 → Ø6 podrá reproducir la huella original si **todos los parámetros pertinentes** vuelven a sus valores iniciales. La función de huella se prepara en la 020; la comprobación de vigencia de trayectorias sigue sin existir en código.

## Arquitectura y configuración observadas

- Visor: `stl.rs`, `preview.rs`, `render.rs`, `stock_render.rs`, `overlay.rs`, `navigation.rs`, `scene_pick.rs`.
- Proyecto: `project/*` y `project_ui.rs`; formato `.lagucam` v2.
- Fresas: `tools/model.rs`, `tools/storage.rs`, `tools_ui.rs`, `tools_profile.rs`; biblioteca independiente de `.lagucam`.
- Aplicación: `main.rs`, `panel_layout.rs`, `options.rs`, `portable.rs`.
- Configuración junto al ejecutable, no AppData: `config/settings.json`, `config/panels.json`, `config/tools.json`.

## Formatos y compatibilidad

**Decisión vigente de desarrollo:** no se exige compatibilidad con versiones anteriores ni migración automática. `.lagucam` v1 se rechaza al abrir; v2 es el único esquema soportado en este corte. Deben documentarse las roturas y no se pueden borrar archivos personales silenciosamente.

## Pruebas y advertencias

- **Validado personalmente por el usuario en una versión anterior:** importar y mover un STL real cercano a 1 GB. No constan métricas sistemáticas de RAM, VRAM, carga o FPS.
- **Código observado:** lector, proyecto v2, biblioteca, UUID, portabilidad, paneles y pivote.
- **No comprobado aquí para `f1e436b`:** ejecución efectiva de `cargo fmt`, Clippy, test, build release, uso gráfico Windows y regresión con STL grande. No atribuir CI sin evidencia de workflows.
- No existe cálculo ni verificación de colisiones o generación de G-code; una rotación del marco CNC **no equivale** a orientación real de la máquina.

## Próxima acción

1. **Realizado:** UUID compartido en biblioteca, operaciones y proyecto (`f1e436b`).
2. **Decidido:** campos de SHA-256 geométrico y separación de parámetros físicos y tecnológicos.
3. **Preparado en ajuste 020:** función de hash canónico/versionado y pruebas; ejecutar Cargo en Windows. Operaciones y `.lagucam` sin cambios.
4. Después, debatir trazabilidad completa de trayectorias, motor geométrico y primera estrategia.

**Histórico:** los documentos `LAGUCAM_004.md`–`LAGUCAM_007.md`, `AUDITORIA.md` y las entradas anteriores de `CHANGELOG.md` describen entregas anteriores; no sustituyen este estado.

# LaguCAM — árbol de proyecto y módulos (estado actual)

**Referencia:** commit `f1e436b976eb6f4f88482b00ae5aeff2761e08ee`, 14-11-2026. Este archivo sustituye una descripción antigua del árbol y del bruto; detalles en `docs/ARCHITECTURE.md`.

```text
src/
  main.rs                 Orquestación UI, archivos, importación y dibujo
  stl.rs                  Lectura/validación STL binario
  preview.rs              Bloques visuales y canal acotado
  render.rs               Dibujo STL mediante OpenGL
  stock_render.rs         Dibujo del bruto
  overlay.rs              Marcadores de cero CNC
  navigation.rs           Acciones y estado de cámara
  scene_pick.rs           Picking VISUAL basado en profundidad GPU
  project/
    mod.rs                Estado del proyecto
    model.rs              Referencia de STL y límites
    stock.rs              Bruto
    origin.rs             Origen CNC y transformaciones
    display.rs            Aspecto del bruto
    selection.rs          Selección de elementos
    operations.rs         Operaciones marcadoras con ToolId UUID
    persistence.rs        .lagucam versión 2
  project_ui.rs           Árbol/propiedades de proyecto
  tools/
    mod.rs                Módulo de herramientas
    model.rs              Cinco geometrías, UUID, validación
    storage.rs            Biblioteca config/tools.json
  tools_ui.rs             Editor y filtros de fresas
  tools_profile.rs        Dibujo 2D de perfiles
  panel_layout.rs         Paneles acoplables y flotantes
  portable.rs             Configuración portable y escritura atómica
  options.rs              Tema y preferencias
```

**No existen todavía** motor CAM, módulo de contactos, trayectorias, verificación de colisiones, máquinas o postprocesadores. El hash SHA-256 de geometría de herramientas está acordado, pero no programado.

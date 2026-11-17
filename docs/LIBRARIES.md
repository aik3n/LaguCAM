# Bibliotecas de herramientas, máquinas y postprocesadores

**Estado de implementación:** consultar [`STATUS.md`](STATUS.md). Este documento describe contratos, biblioteca y datos físicos; no fija un SHA vivo.

Las capturas de DeskProto compartidas por el usuario son **referencias funcionales**, no instrucciones de copiar su diseño ni prueba de que sus valores sean capacidades certificadas de una máquina.

## 1. Principio de separación

| Entidad | Define | No debe definir |
| --- | --- | --- |
| **Herramienta** | Perfil geométrico, dimensiones, corte, mango, límites propios, parámetros recomendados | Número de almacén universal, dialecto G-code |
| **Máquina** | Recorridos/capacidades mecánicas, husillo, límites, modo de cambio | Algoritmo de contacto ni formato de malla |
| **Postprocesador** | Dialecto, cabecera, pie, movimientos, RPM, avances, cambio y formato de archivos | Geometría de la herramienta, capacidades mecánicas no declaradas |
| **Proyecto / setup** | Modelo, bruto, origen, máquina/configuración elegida, herramientas asignadas, operaciones | Reescribir definiciones globales sin consentimiento |
| **Operación** | Estrategia, herramienta del proyecto, feeds/speeds específicos y parámetros | Formato final de G-code |

## 2. Biblioteca de herramientas — UX

Una colección permanente de herramientas, **sin carpetas obligatorias**. **Ya se puede buscar y filtrar** por nombre, geometría y diámetro; favoritas y recientes son mejoras recomendadas. La operación dispone de selector de herramientas de la biblioteca; el cálculo acepta solo geometrías soportadas por el solver, sin aproximaciones ocultas.

El editor implementado tiene **lista/filtros, propiedades y dibujo técnico parametrizado del perfil**. Debe actualizar la vista previa con los datos reales y mostrar cotas, no inventar geometría aproximada no declarada.

Ya se implementaron crear, duplicar, editar, eliminar, buscar y ordenar, además de seleccionar desde operaciones y resolver la referencia UUID en `cam_adapter.rs`. La identidad UUID y revisión existen; no hay snapshot editable de herramienta en el proyecto.

### Geometrías candidatas iniciales

1. Fresa plana.
2. Fresa esférica (*ball nose*).
3. Fresa toroidal (*bull nose*, con radio de esquina).
4. Cónica en V.
5. Cónica con punta plana.
6. Cónica con punta esférica.

Las seis familias están representadas en el modelo de biblioteca inspeccionado. El **contacto CAM** está implementado para plana, esférica y toroidal; las cónicas permanecen sin solver. «Esférica con punta plana» es otra geometría que no se debe confundir con la toroidal.

### Campos conceptuales

- Identificación: ID estable, nombre, tipo, notas, fabricante/referencia opcionales.
- Geometría de corte: diámetros, radio de esquina o punta, ángulos, longitud de filo/corte; **campos dependientes del tipo**.
- Cuerpo no cortante: diámetro de mango, cuello, longitud libre y longitud total cuando se conozcan.
- Montaje: salida de pinza, portaherramientas y cotas relevantes para colisiones; separar de la herramienta física.
- Tecnología: número de filos, RPM y avances recomendados, material objetivo, límites; **no confundir parámetros sugeridos con seguridad certificada**.
- Metadatos: unidades, fecha/revisión y fuente de medición.

Precisar si el ángulo cónico es **total** o **semiángulo respecto al eje**. No inferir si la punta Ø0,1 es plana o esférica; deben ser modelos diferentes si proceden.

### Reutilización por proyecto

**Decisión D-024:** la operación apunta al UUID de la **biblioteca global** sin instantánea editable persistente. El adaptador del CAM ya obtiene geometría y SHA-256 durante el cálculo; cambiar parámetros físicos o eliminar la herramienta invalida la vista previa. Esto **no** acredita vigencia del resto de dependencias ni autoriza exportar G-code.

## 3. Biblioteca de máquinas

La pantalla DeskProto mostraba un perfil llamado **aiken**. Valores de referencia a **confirmar físicamente**:

| Parámetro mostrado | Valor |
| --- | --- |
| Recorrido X / Y / Z | 300 / 400 / 100 mm |
| Diámetro de pinza/representación | 20 mm |
| Avance mínimo / defecto / máximo | 60 / 1200 / 1200 mm/min |
| Husillo mínimo / defecto / máximo | 1000 / 14000 / 14000 rpm |
| «Número de herramientas» | 8, **no implica ATC** |
| Modo de cambio real del equipo | **Manual**, confirmado por el usuario |
| Control actual | **FluidNC**, confirmado por el usuario |

La biblioteca debe admitir otras máquinas con cambio automático. Un perfil de máquina podrá contener dimensiones útiles, recorridos, límites positivos/negativos, velocidades, husillo, homing, volúmenes prohibidos y capacidades de cambio. **No asumir que recorridos XYZ describen por sí solos el espacio seguro en G53**.

Las unidades, límites y offsets reales deben verificarse contra la configuración concreta del controlador antes de habilitar un post operativo.

## 4. Biblioteca de postprocesadores

Primer objetivo **FluidNC**. La captura de DeskProto muestra una selección «Sam GRBL (Arduino) - mm», pero esto es el perfil que DeskProto ofrecía; **no establece que dicho post sea la especificación correcta de FluidNC**.

Un postprocesador futuro incluirá, según necesidad y compatibilidad:

- Identidad y extensión del programa; formato decimal, precisión y finales de línea.
- Cabecera y pie: unidades, modo absoluto/relativo, offsets, plano, arranque/parada de husillo y fin.
- Movimientos: `G0`, `G1`, arcos si se soportan y verifican, retracciones.
- Avances y RPM, modos soportados, límites y validación.
- Cambios manuales/automáticos, comandos de parada y protocolo de recuperación seguro.
- Numeración de líneas y tamaño máximo solamente si el control/flujo los requiere.

La configuración real de FluidNC y del firmware debe documentarse antes de generar código ejecutable: homing, posiciones de máquina, soft limits, herramientas y cómo realizar el cambio manual y el nuevo cero de longitud.

**Invariante de seguridad:** no convertir un ejemplo de cabecera/pie en código listo para ejecutar sin ensayo offline, revisión humana y verificación con la máquina concreta.

## 5. Biblioteca y configuración del proyecto

El proyecto `.lagucam` **v2** existe y sus operaciones admiten ya una referencia UUID textual a la biblioteca global. **No se guardarán copias editables de fresa.** Las trayectorias futuras registrarán huella y dependencias para evitar resultados obsoletos. Se puede seleccionar otra máquina en el futuro, pero no reutilizar automáticamente un G-code previamente emitido sin recomprobación.

La decisión de permitir varias máquinas por proyecto está abierta. La propuesta inicial fue una sola máquina por proyecto; no se ha ratificado.


## 6. Implementación de la biblioteca global (contratos)

- **Implementado:** `tools/model.rs` contiene seis variantes, con geometría en `f64`; `tools/storage.rs` guarda biblioteca versionada (`tools.json`, versión 1). `tools_ui.rs` ofrece editor/filtros y `tools_profile.rs` dibuja el perfil técnico. Desde `f1e436b`, `ToolId = String` comparte UUID entre biblioteca, operaciones y persistencia `.lagucam` v2.
- **Portabilidad:** `config/tools.json` vive en la carpeta `config/` **junto al ejecutable**, no en AppData.
- **Semántica:** longitud total de la fresa desde punta a extremo de mango representado **no significa** salida de pinza. No se ha modelado el portaherramientas ni la colisión real.
- **Geometrías:** plana; esférica (radio Ø/2); toroidal con radio de esquina; V con ángulo **incluido total**; cónica de punta plana y cónica de punta esférica. Perfil 2D y solver matemático no son intercambiables.
- **Implementado parcialmente:** selector por operación, resolución UUID y comprobación SHA-256 de herramienta en previsualizaciones. **No implementado:** invalidación por todas las dependencias, verificación de colisiones, perfiles de máquina, post FluidNC ni trayectorias seguras para CNC.

## 7. Huella geométrica SHA-256: integración CAM parcial

`src/tools/hash.rs` calcula **SHA-256** sobre una codificación **binaria canónica y versionada** de la geometría efectiva, no sobre el JSON de presentación. El hash no se guarda por herramienta como dato editable: se recalcula a partir de su definición cuando sea necesario.

| Incluir | Excluir |
| --- | --- |
| Tipo; diámetros, radios, ángulos; longitud de corte; diámetro de mango; longitud total; futuras cotas físicas relevantes | UUID, revisión, nombre, notas, referencia comercial, RPM y avances recomendados |

Dos geometrías idénticas deben producir el mismo hash incluso con revisiones distintas. En la secuencia **Ø6 → Ø4 → Ø6**, el hash podrá volver al original si se restablecen todos los parámetros pertinentes. Modificar el nombre no debe provocar recálculo geométrico; cambiar feeds/speeds puede exigir regenerar G-code. La igualdad de huella de herramienta, por sí sola, **no demuestra vigencia de una trayectoria**: habrá que contrastar geometría fuente, bruto, origen, estrategia y versión de cálculo. La vigencia por herramienta ya existe en la vista previa; la vigencia **completa** de modelo, bruto, setup, estrategia y programa sigue pendiente. El TEST obtiene la geometría CAM de la biblioteca por UUID, no del diámetro serializado legado.

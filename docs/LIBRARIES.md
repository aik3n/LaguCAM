# Bibliotecas de herramientas, máquinas y postprocesadores

**Estado:** diseño funcional acordado con componentes aún pendientes · **Fecha:** 2026-11-14

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

Una colección permanente de herramientas, **sin carpetas obligatorias**. Se podrá buscar y filtrar por nombre, geometría, diámetro; favoritas y recientes son mejoras recomendadas. La operación abrirá un selector ágil, preferentemente con herramientas que el algoritmo acepte, sin esconder compatibilidades relevantes.

El editor completo tendrá: **lista/selector, propiedades y dibujo técnico parametrizado del perfil**. Debe actualizar la vista previa con los datos reales y mostrar cotas, no inventar geometría aproximada no declarada.

Operaciones previstas: crear, copiar, editar, eliminar/archivar, buscar, elegir, y comprobar referencias desde proyectos. Identidad estable de herramienta y revisión o snapshot.

### Geometrías candidatas iniciales

1. Fresa plana.
2. Fresa esférica (*ball nose*).
3. Cónica en V.
4. Cónica con punta plana.
5. Cónica con punta esférica.

**Pendiente de cerrar** la lista exacta. Fresa toroidal (*bull nose* / radio de esquina) y «esférica con punta plana» se contemplan para el futuro; no deben confundirse entre sí.

### Campos conceptuales

- Identificación: ID estable, nombre, tipo, notas, fabricante/referencia opcionales.
- Geometría de corte: diámetros, radio de esquina o punta, ángulos, longitud de filo/corte; **campos dependientes del tipo**.
- Cuerpo no cortante: diámetro de mango, cuello, longitud libre y longitud total cuando se conozcan.
- Montaje: salida de pinza, portaherramientas y cotas relevantes para colisiones; separar de la herramienta física.
- Tecnología: número de filos, RPM y avances recomendados, material objetivo, límites; **no confundir parámetros sugeridos con seguridad certificada**.
- Metadatos: unidades, fecha/revisión y fuente de medición.

Precisar si el ángulo cónico es **total** o **semiángulo respecto al eje**. No inferir si la punta Ø0,1 es plana o esférica; deben ser modelos diferentes si proceden.

### Reutilización por proyecto

Cada operación futura referenciará directamente el **UUID de una herramienta de la biblioteca global**, sin copia de geometría, RPM ni avances en el proyecto. Las propiedades actuales de esa herramienta se resolverán al usarla. Si el UUID no existe, la operación mostrará un error y no podrá calcularse. Una modificación de la fresa invalida cualquier trayectoria anterior dependiente; nunca se reutilizará G-code obsoleto.

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

El proyecto `.lagucam` ya existe. Cuando haya operaciones CAM, cada una registrará únicamente el UUID de su herramienta en la biblioteca global; no incluirá copias ocultas de sus dimensiones ni de sus parámetros tecnológicos. Si una herramienta cambia o desaparece, la operación requerirá revisión o recálculo. No reutilizar automáticamente G-code anterior.

La decisión de permitir varias máquinas por proyecto está abierta. La propuesta inicial fue una sola máquina por proyecto; no se ha ratificado.

## 6. Primera biblioteca global: convención geométrica implementada

**Alcance:** los cinco tipos solicitados para esta entrega, sin añadir toroidales ni funciones CAM. Todas las cotas están en milímetros y se almacenan como `f64`. Se mide `z` desde el extremo inferior de la herramienta hacia el mango; la punta comienza en `z = 0`. El diámetro de corte es el **máximo diámetro del tramo cortante**, no el del mango.

| Geometría | Parámetros de corte | Perfil geométrico |
| --- | --- | --- |
| Plana | Ø de corte, longitud de corte | Cilindro cortante con extremo plano |
| Esférica | Ø de corte, longitud de corte | Hemisferio de radio Ø/2, seguido de cilindro cortante |
| Cónica en V | Ø máximo de corte, **ángulo total** | Cono con ápice matemático en z=0, hasta Ø máximo |
| Cónica con punta plana | Ø máximo, Ø de punta plana, **ángulo total** | Disco frontal plano y flanco cónico hasta Ø máximo |
| Cónica con punta esférica | Ø máximo, **radio R de una esfera de punta**, **ángulo total** | Casquete esférico unido tangencialmente al flanco cónico; no es un radio toroidal |

Para cada cono, α = ángulo total/2 (semiángulo respecto al eje). Si R es el radio de la punta esférica, la tangencia sucede en `r = R cos α` y `z = R(1 − sin α)`. Desde esa tangencia el radio crece como `tan α` por unidad axial. El radio máximo `D/2` debe exceder `R cos α`, y la longitud cortante debe alcanzar la altura necesaria. Para la punta plana, la altura cónica es `(D − d_punta)/(2 tan α)`; para V pura es `D/(2 tan α)`. Si sobra longitud de corte, se representa un tramo cilíndrico cortante a Ø máximo.

La parte no cortante se modela inicialmente con **diámetro de mango** y **longitud total** desde la punta axial hasta el extremo superior del mango representado. Debe ser mayor o igual a la longitud cortante. **No es la salida de pinza** y no modela el portaherramientas. Los registros anteriores con `free_length_mm` se leen sin alterar su número y pasan a `total_length_mm` al guardar; revisar esas longitudes si se introdujeron como salida de pinza. No se modelan cuello estrechado ni portaherramientas; se añadirán antes de evaluar colisiones. RPM, avance y número de filos son opcionales.

El archivo global está en `%APPDATA%\LaguCAM\tools.json` (Windows) o el directorio de configuración equivalente (Linux/macOS). Esquema versión 1; UUIDv4 y revisión de herramienta permiten identificar cambios y marcar resultados como obsoletos. No se crean instantáneas de herramienta en los proyectos.

Esta convención corresponde a la primera implementación solicitada; no implica aprobar automáticamente decisiones abiertas ni certificar contactos o colisiones.

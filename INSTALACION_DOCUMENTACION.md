# Integración de la actualización documental

**Código de referencia:** `f1e436b976eb6f4f88482b00ae5aeff2761e08ee` (14-11-2026). **Solo documentación; ningún archivo Rust ha sido cambiado.**

La conexión GitHub permitió leer los documentos, pero rechazó la escritura con **403 Resource not accessible by integration**. Este ZIP es el conjunto de Markdown preparado para revisión e incorporación mediante el chat de programación o Git local.

## Aplicación

1. Comprobar el HEAD del repositorio. Si avanzó respecto al SHA de referencia, **comparar y fusionar** los documentos, no sobrescribir ciegamente cambios posteriores.
2. Copiar `README.md`, `AGENTS.md`, `PROJECT_TREE.md` y los archivos de `docs/` a las rutas equivalentes del repositorio.
3. **No modificar código ni ejemplos** al instalar estos documentos. Mantener `AUDITORIA.md`, `DEPENDENCIAS.md` y `LAGUCAM_004.md`–`LAGUCAM_007.md` como referencias históricas; su contenido no fue reescrito en este paquete.
4. Revisar cambios con `git diff`, corregir conflictos y crear **un commit exclusivamente documental**, si se desea. Actualizar en el mensaje de commit el SHA resultante; no inventarlo en los archivos.

## Reglas consolidadas

- `.lagucam` v2; UUID `ToolId` compartido; biblioteca global, sin snapshot persistente de herramienta en proyecto.
- Sin compatibilidad ni migraciones obligatorias de formatos durante desarrollo; no borrar datos personales silenciosamente.
- Huella SHA-256 de geometría: decisión aceptada, **pendiente de implementación**. Debe usar datos binarios canónicos y versionados; excluir UUID, nombre, revisión, notas, RPM y avance.
- Siguiente desarrollo recomendado solo si el usuario lo solicita: función de hash y tests, sin inventar operaciones CAM ni modificar `.lagucam` todavía.
- No se realizaron pruebas Rust, Windows/OpenGL ni CNC en la actualización documental.

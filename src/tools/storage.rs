//! Persistencia global independiente de los proyectos `.lagucam`.
//! El fichero anterior no se toca si fallan serialización, escritura o validación.
use super::model::ToolLibrary;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_LIBRARY_BYTES: u64 = 32 * 1024 * 1024;

/// Windows: %APPDATA%\LaguCAM\tools.json
/// macOS: ~/Library/Application Support/LaguCAM/tools.json
/// Linux/BSD: $XDG_CONFIG_HOME/lagucam/tools.json o ~/.config/lagucam/tools.json
pub fn config_path() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let dir = std::env::var_os("APPDATA").ok_or("No se ha encontrado APPDATA")?;
        Ok(PathBuf::from(dir).join("LaguCAM").join("tools.json"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("No se ha encontrado HOME")?;
        Ok(PathBuf::from(home).join("Library/Application Support/LaguCAM/tools.json"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".config")))
            .ok_or("No se ha encontrado directorio de configuración del usuario")?;
        Ok(base.join("lagucam").join("tools.json"))
    }
}

/// Fichero ausente en primera ejecución: biblioteca vacía SOLO en memoria.
/// JSON corrupto: error; no se trunca ni reescribe silenciosamente.
pub fn load(path: &Path) -> Result<ToolLibrary, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(ToolLibrary::default()),
        Err(err) => return Err(format!("No se puede abrir biblioteca: {err}")),
    };
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_LIBRARY_BYTES {
        return Err("La biblioteca supera el tamaño máximo permitido".into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("La biblioteca supera el tamaño máximo permitido".into());
    }
    let library: ToolLibrary =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON de biblioteca inválido: {e}"))?;
    library.validate()?;
    Ok(library)
}

/// Guardado sin modificar la biblioteca anterior hasta que el fichero temporal
/// esté íntegro y sincronizado. Windows: ReplaceFileW; Unix: rename.
pub fn save(path: &Path, library: &ToolLibrary) -> Result<(), String> {
    library.validate()?;
    let bytes = serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("La biblioteca supera el tamaño máximo permitido".into());
    }
    let parent = path.parent().ok_or("Ruta de biblioteca sin directorio")?;
    fs::create_dir_all(parent).map_err(|e| format!("No se puede crear configuración: {e}"))?;
    let tmp = parent.join(format!(".lagucam-tools-{}.tmp", Uuid::new_v4()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("No se puede crear fichero temporal: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("No se puede escribir biblioteca: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("No se pudo sincronizar la escritura: {e}"))?;
        drop(file);
        replace_file(&tmp, path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *const std::ffi::c_void,
            reserved: *const std::ffi::c_void,
        ) -> i32;
    }
    if target.exists() {
        let old: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let new: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            ReplaceFileW(
                old.as_ptr(),
                new.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if result == 0 {
            return Err(format!(
                "No se pudo sustituir la biblioteca: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    } else {
        fs::rename(temp, target).map_err(|e| format!("No se pudo instalar la biblioteca: {e}"))
    }
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target).map_err(|e| format!("No se pudo sustituir la biblioteca: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::model::{Tool, ToolGeometry, ToolKind};

    fn valid_library() -> ToolLibrary {
        let mut lib = ToolLibrary::default();
        let mut t = Tool::blank(ToolKind::Flat);
        t.name = "Plana".into();
        t.geometry = ToolGeometry::Flat {
            cutting_diameter_mm: 6.0,
        };
        t.cutting_length_mm = 12.0;
        t.total_length_mm = 30.0;
        t.shank_diameter_mm = 6.0;
        lib.add(t).unwrap();
        lib
    }

    fn directory() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lagucam-tools-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_run_empty_and_roundtrip() {
        let dir = directory();
        let path = dir.join("tools.json");
        assert!(load(&path).unwrap().tools.is_empty());
        let lib = valid_library();
        save(&path, &lib).unwrap();
        assert_eq!(load(&path).unwrap(), lib);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupted_and_future_version_are_rejected() {
        let dir = directory();
        let path = dir.join("tools.json");
        fs::write(&path, b"{invalid").unwrap();
        assert!(load(&path).is_err());
        let mut lib = valid_library();
        lib.version += 1;
        fs::write(&path, serde_json::to_vec(&lib).unwrap()).unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refused_file_replacement_preserves_directory_and_data() {
        let dir = directory();
        let protected = dir.join("existing_directory");
        fs::create_dir(&protected).unwrap();
        fs::write(protected.join("keep.txt"), b"original").unwrap();
        assert!(save(&protected, &valid_library()).is_err());
        assert_eq!(fs::read(protected.join("keep.txt")).unwrap(), b"original");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_write_preserves_previous_contents() {
        let dir = directory();
        let path = dir.join("tools.json");
        let lib = valid_library();
        save(&path, &lib).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = lib.clone();
        invalid.tools[0].shank_diameter_mm = f64::NAN;
        assert!(save(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
}

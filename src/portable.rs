//! Configuración portable: misma carpeta que el ejecutable, sin AppData ni Registro.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

fn path_for(executable: &Path, file: &str) -> Result<PathBuf, String> {
    if !matches!(
        file,
        "settings.json"
            | "panels.json"
            | "tools.json"
            | "recent_projects.json"
            | "postprocessors.json"
            | "postprocessor.json"
            | "machine.json"
    ) {
        return Err("Nombre de configuración no permitido".into());
    }
    let parent = executable
        .parent()
        .ok_or("No se puede determinar la carpeta del ejecutable")?;
    Ok(parent.join("config").join(file))
}

pub fn config_file(file: &str) -> Result<PathBuf, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("No se puede localizar el ejecutable: {e}"))?;
    path_for(&executable, file)
}

/// Escritura segura: el fichero anterior no se sustituye hasta validar
/// escritura y sincronización del temporal. Nunca se migra desde AppData.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Ruta de configuración sin carpeta")?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("No se puede crear la carpeta portable: {e}"))?;
    let temp = parent.join(format!(".lagucam-config-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("No se puede crear el archivo temporal: {e}"))?;
        handle.write_all(bytes).map_err(|e| e.to_string())?;
        handle.sync_all().map_err(|e| e.to_string())?;
        drop(handle);
        replace_file(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
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
                "No se pudo actualizar la configuración: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    } else {
        fs::rename(temp, target).map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_share_a_portable_config_directory() {
        let executable = Path::new("LaguCAM").join("LaguCAM.exe");
        for name in [
            "settings.json",
            "panels.json",
            "tools.json",
            "recent_projects.json",
            "postprocessors.json",
            "postprocessor.json",
            "machine.json",
        ] {
            assert_eq!(
                path_for(&executable, name).unwrap(),
                Path::new("LaguCAM").join("config").join(name)
            );
        }
        assert!(path_for(&executable, "../unsafe.json").is_err());
        assert!(path_for(&executable, "legacy.json").is_err());
    }

    #[test]
    fn atomic_replacement_preserves_one_complete_file() {
        let dir = std::env::temp_dir().join(format!("lagucam-portable-test-{}", Uuid::new_v4()));
        let path = dir.join("config").join("settings.json");
        atomic_write(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        atomic_write(&path, b"second version").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second version");
        fs::remove_dir_all(dir).unwrap();
    }
}

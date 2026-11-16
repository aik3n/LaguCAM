//! Historial portable de proyectos `.lagucam` abiertos o guardados correctamente.
//! No forma parte del documento CAM ni modifica proyectos.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

const MAX_RECENT: usize = 5;
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct RecentFile {
    version: u32,
    paths: Vec<PathBuf>,
}

pub struct RecentProjects {
    path: Option<PathBuf>,
    paths: Vec<PathBuf>,
    error: Option<String>,
}

impl RecentProjects {
    pub fn load() -> Self {
        let path = match crate::portable::config_file("recent_projects.json") {
            Ok(path) => path,
            Err(error) => {
                return Self {
                    path: None,
                    paths: Vec::new(),
                    error: Some(error),
                };
            }
        };
        let mut recent = Self {
            path: Some(path.clone()),
            paths: Vec::new(),
            error: None,
        };
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<RecentFile>(&bytes) {
                Ok(stored) if stored.version == VERSION => {
                    for candidate in stored.paths {
                        if is_project(&candidate)
                            && candidate.is_absolute()
                            && !recent
                                .paths
                                .iter()
                                .any(|existing| same_path(existing, &candidate))
                        {
                            recent.paths.push(candidate);
                            if recent.paths.len() == MAX_RECENT {
                                break;
                            }
                        }
                    }
                }
                Ok(_) => {
                    recent.error = Some("Versión desconocida de config/recent_projects.json".into())
                }
                Err(error) => {
                    recent.error = Some(format!("Historial de recientes no válido: {error}"))
                }
            },
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => recent.error = Some(format!("No se puede leer el historial: {error}")),
        }
        recent
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn record(&mut self, path: &Path) -> Result<(), String> {
        if !is_project(path) {
            return Err("Solo se registran proyectos .lagucam".into());
        }
        // Las rutas relativas se resuelven en el momento del guardado/apertura,
        // nunca al cargar el historial desde otra carpeta de trabajo.
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        self.save(promote(&self.paths, absolute))
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.save(Vec::new())
    }

    fn save(&mut self, paths: Vec<PathBuf>) -> Result<(), String> {
        let result = (|| -> Result<(), String> {
            let target = self
                .path
                .as_ref()
                .ok_or("Configuración portable no disponible")?;
            let bytes = serde_json::to_vec_pretty(&RecentFile {
                version: VERSION,
                paths: paths.clone(),
            })
            .map_err(|e| e.to_string())?;
            crate::portable::atomic_write(target, &bytes)
        })();
        match result {
            Ok(()) => {
                self.paths = paths;
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }
}

fn is_project(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lagucam"))
}

fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn promote(current: &[PathBuf], path: PathBuf) -> Vec<PathBuf> {
    let mut result = vec![path.clone()];
    for previous in current {
        if !same_path(previous, &path) && result.len() < MAX_RECENT {
            result.push(previous.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_no_duplicates_limit_five() {
        let mut list = Vec::new();
        for n in 0..7 {
            list = promote(&list, PathBuf::from(format!("C:/files/p{n}.lagucam")));
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], PathBuf::from("C:/files/p6.lagucam"));
        assert_eq!(list[4], PathBuf::from("C:/files/p2.lagucam"));
        list = promote(&list, PathBuf::from("C:/files/p3.lagucam"));
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], PathBuf::from("C:/files/p3.lagucam"));
        assert_eq!(
            list.iter()
                .filter(|p| *p == &PathBuf::from("C:/files/p3.lagucam"))
                .count(),
            1
        );
    }

    #[test]
    fn project_extension_and_serialization() {
        assert!(is_project(Path::new("piece.LAGUCAM")));
        assert!(!is_project(Path::new("piece.stl")));
        let file = RecentFile {
            version: VERSION,
            paths: vec![PathBuf::from("C:/proyectos/uno.lagucam")],
        };
        let bytes = serde_json::to_vec(&file).unwrap();
        let parsed: RecentFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.version, VERSION);
        assert_eq!(parsed.paths, file.paths);
    }
}

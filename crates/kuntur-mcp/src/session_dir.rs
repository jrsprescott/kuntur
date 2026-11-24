//! Carpeta temporal de cada sesión: `%LOCALAPPDATA%\Kuntur\mcp\<pid>-<sesión>\` con los archivos
//! generados para los CLI y la subcarpeta `salida\` (capturas de los MCP). Se borra al cerrar la
//! sesión; al arrancar, Kuntur borra las de procesos que ya no existen.

use std::path::{Path, PathBuf};

/// Subcarpeta donde los MCP dejan capturas o archivos (se abren en la vista previa).
pub const OUTPUT_DIR: &str = "salida";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDirs {
    root: PathBuf,
    pid: u32,
}

impl SessionDirs {
    pub fn new(root: PathBuf, pid: u32) -> Self {
        Self { root, pid }
    }

    /// `%LOCALAPPDATA%\Kuntur\mcp` (o la carpeta temporal si no existe la variable).
    pub fn default_root() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Kuntur")
            .join("mcp")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn session(&self, id: u64) -> PathBuf {
        self.root.join(format!("{}-{id}", self.pid))
    }

    pub fn output(&self, id: u64) -> PathBuf {
        self.session(id).join(OUTPUT_DIR)
    }

    /// Crea la carpeta de la sesión y su `salida\`.
    pub fn create(&self, id: u64) -> std::io::Result<PathBuf> {
        let dir = self.session(id);
        std::fs::create_dir_all(dir.join(OUTPUT_DIR))?;
        Ok(dir)
    }

    /// Borra la carpeta de la sesión (al cerrarla).
    pub fn remove(&self, id: u64) {
        let _ = std::fs::remove_dir_all(self.session(id));
    }

    /// Borra las carpetas de otros procesos que ya no corren. Devuelve cuántas borró.
    pub fn clean_stale(&self, alive: impl Fn(u32) -> bool) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut removed = 0;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(pid) = name.split_once('-').and_then(|(p, _)| p.parse::<u32>().ok()) else {
                continue;
            };
            if pid != self.pid && !alive(pid) && std::fs::remove_dir_all(e.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crea_borra_y_limpia_huerfanas() {
        let root = std::env::temp_dir().join(format!("kuntur-mcp-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let me = SessionDirs::new(root.clone(), 100);
        let dir = me.create(3).unwrap();
        assert!(dir.ends_with("100-3"));
        assert!(me.output(3).is_dir());
        let other_alive = SessionDirs::new(root.clone(), 200);
        let other_dead = SessionDirs::new(root.clone(), 300);
        other_alive.create(1).unwrap();
        other_dead.create(1).unwrap();
        std::fs::create_dir_all(root.join("no-es-de-kuntur")).unwrap();
        assert_eq!(me.clean_stale(|pid| pid == 200), 1);
        assert!(other_alive.session(1).exists());
        assert!(!other_dead.session(1).exists());
        assert!(root.join("no-es-de-kuntur").exists());
        me.remove(3);
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

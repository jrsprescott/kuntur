//! Capturas y archivos que dejan los MCP en la carpeta `salida\` de la sesión (§11): se detectan
//! los nuevos o cambiados para abrirlos en la vista previa (que ya se recarga sola).
//!
//! Se mira por sondeo (cada pocos segundos, en el ciclo del monitor) y solo el primer nivel de la
//! carpeta: barato y sin hilos extra. Los archivos temporales (`~…`, `.tmp`, `.part`) se ignoran.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Default)]
pub struct OutputWatcher {
    seen: HashMap<PathBuf, (SystemTime, u64)>,
}

fn ignored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('~')
        || lower.starts_with('.')
        || lower.ends_with(".tmp")
        || lower.ends_with(".part")
        || lower.ends_with(".crdownload")
}

impl OutputWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lo que ya existe no cuenta como nuevo (al activar la vigilancia).
    pub fn prime(&mut self, dir: &Path) {
        let _ = self.scan(dir);
    }

    /// Archivos nuevos o cambiados desde la última vez, del más viejo al más nuevo.
    pub fn scan(&mut self, dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut fresh: Vec<(SystemTime, PathBuf)> = Vec::new();
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() || meta.len() == 0 || ignored(&e.file_name().to_string_lossy()) {
                continue;
            }
            let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
            let path = e.path();
            if self.seen.get(&path) != Some(&stamp) {
                self.seen.insert(path.clone(), stamp);
                fresh.push((stamp.0, path));
            }
        }
        fresh.sort();
        fresh.into_iter().map(|(_, p)| p).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_nuevos_y_cambiados_e_ignora_temporales() {
        let dir = std::env::temp_dir().join(format!("kuntur-mcp-salida-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vieja.png"), b"1").unwrap();
        let mut w = OutputWatcher::new();
        w.prime(&dir);
        assert!(w.scan(&dir).is_empty());

        std::fs::write(dir.join("escena.png"), b"png").unwrap();
        std::fs::write(dir.join("escena.png.tmp"), b"x").unwrap();
        std::fs::write(dir.join("vacia.png"), b"").unwrap();
        let new = w.scan(&dir);
        assert_eq!(new, [dir.join("escena.png")]);
        assert!(w.scan(&dir).is_empty(), "una sola vez");

        // Cambió (otro tamaño): vuelve a aparecer.
        std::fs::write(dir.join("escena.png"), b"png2").unwrap();
        assert_eq!(w.scan(&dir), [dir.join("escena.png")]);
        assert!(w.scan(&dir.join("no-existe")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

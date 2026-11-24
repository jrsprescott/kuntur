//! Recarga automática: vigila la **carpeta** de cada archivo abierto (los editores y los agentes
//! suelen guardar escribiendo un temporal y renombrándolo, y eso no se ve vigilando el archivo) y
//! avisa con la ruta que cambió. Quien recibe el aviso filtra los archivos que le interesan.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub struct FileWatcher {
    inner: RecommendedWatcher,
    /// Carpeta vigilada → cuántos archivos abiertos hay en ella.
    dirs: HashMap<PathBuf, usize>,
}

impl FileWatcher {
    /// `on_change` se llama en el hilo de `notify` con cada ruta creada, modificada o renombrada.
    pub fn new(on_change: impl Fn(PathBuf) + Send + 'static) -> notify::Result<Self> {
        let inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else {
                return;
            };
            if matches!(ev.kind, EventKind::Access(_)) {
                return;
            }
            for p in ev.paths {
                on_change(p);
            }
        })?;
        Ok(Self {
            inner,
            dirs: HashMap::new(),
        })
    }

    /// Empieza a vigilar el archivo (su carpeta, sin subcarpetas).
    pub fn watch(&mut self, file: &Path) -> notify::Result<()> {
        let Some(dir) = file.parent().map(Path::to_path_buf) else {
            return Ok(());
        };
        if let Some(n) = self.dirs.get_mut(&dir) {
            *n += 1;
            return Ok(());
        }
        self.inner.watch(&dir, RecursiveMode::NonRecursive)?;
        self.dirs.insert(dir, 1);
        Ok(())
    }

    /// Deja de vigilarlo (la carpeta se suelta cuando no queda ningún archivo abierto en ella).
    pub fn unwatch(&mut self, file: &Path) {
        let Some(dir) = file.parent() else {
            return;
        };
        if let Some(n) = self.dirs.get_mut(dir) {
            *n -= 1;
            if *n == 0 {
                self.dirs.remove(dir);
                let _ = self.inner.unwatch(dir);
            }
        }
    }

    /// Carpetas vigiladas ahora.
    pub fn watched_dirs(&self) -> usize {
        self.dirs.len()
    }
}

/// ¿El aviso es del archivo? (mismas mayúsculas o no: Windows no las distingue).
pub fn same_file(a: &Path, b: &Path) -> bool {
    a == b || a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn avisa_al_modificar_y_suelta_la_carpeta() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("vigilado.md");
        std::fs::write(&file, "uno").unwrap();
        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(move |p| {
            let _ = tx.send(p);
        })
        .unwrap();
        w.watch(&file).unwrap();
        w.watch(&dir.path().join("otro.md")).unwrap();
        assert_eq!(w.watched_dirs(), 1);

        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(&file, "dos").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while std::time::Instant::now() < deadline {
            if let Ok(p) = rx.recv_timeout(Duration::from_millis(200))
                && same_file(&p, &file)
            {
                seen = true;
                break;
            }
        }
        assert!(seen, "no llegó el aviso del cambio");

        w.unwatch(&file);
        assert_eq!(w.watched_dirs(), 1);
        w.unwatch(&dir.path().join("otro.md"));
        assert_eq!(w.watched_dirs(), 0);
    }

    #[test]
    fn misma_ruta_sin_distinguir_mayusculas() {
        assert!(same_file(Path::new(r"C:\a\B.md"), Path::new(r"c:\A\b.MD")));
        assert!(!same_file(Path::new(r"C:\a\b.md"), Path::new(r"C:\a\c.md")));
    }
}

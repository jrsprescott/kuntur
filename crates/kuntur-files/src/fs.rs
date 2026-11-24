//! Sistema de archivos del panel "Archivos": el disco local ([`LocalFs`]), un servidor por SFTP
//! (`kuntur_ssh::SftpFs`) o uno en memoria ([`crate::MemoryFs`], pruebas y humo).
//!
//! La interfaz es **síncrona**: la app la llama fuera del hilo de la UI (lecturas en segundo
//! plano y el hilo de transferencias). Las rutas son texto en el estilo de [`PathStyle`].

use std::io::{self, Read, Write};
use std::path::Path;

use crate::path::PathStyle;
use crate::{FileEntry, Listing, list_dir};

/// Operaciones que el árbol y las transferencias necesitan.
pub trait FileSystem: Send + Sync {
    /// Estilo de las rutas (Windows o POSIX).
    fn style(&self) -> PathStyle;

    /// `None` = local; `Some("usuario@host")` = remoto (para la etiqueta del panel).
    fn remote_label(&self) -> Option<String>;

    /// Carpeta inicial cuando la sesión no informó ninguna (la personal).
    fn home(&self) -> io::Result<String>;

    /// Lista una carpeta: carpetas primero, por nombre; como mucho [`crate::MAX_ENTRIES`].
    fn list(&self, dir: &str) -> io::Result<Listing>;

    /// Datos de una ruta; `Ok(None)` si no existe.
    fn stat(&self, path: &str) -> io::Result<Option<FileEntry>>;

    /// Abre un archivo para leerlo por bloques.
    fn open_read(&self, path: &str) -> io::Result<Box<dyn Read + Send>>;

    /// Crea (o vacía) un archivo para escribirlo por bloques. `flush` confirma lo escrito.
    fn create(&self, path: &str) -> io::Result<Box<dyn Write + Send>>;

    fn create_dir(&self, path: &str) -> io::Result<()>;

    fn rename(&self, from: &str, to: &str) -> io::Result<()>;

    fn remove_file(&self, path: &str) -> io::Result<()>;

    /// Borra una carpeta **vacía**.
    fn remove_dir(&self, path: &str) -> io::Result<()>;

    // ── Con implementación por defecto ──────────────────────────────────────

    fn join(&self, dir: &str, name: &str) -> String {
        self.style().join(dir, name)
    }

    fn parent(&self, path: &str) -> Option<String> {
        self.style().parent(path)
    }

    fn file_name(&self, path: &str) -> Option<String> {
        self.style().file_name(path)
    }

    fn exists(&self, path: &str) -> io::Result<bool> {
        Ok(self.stat(path)?.is_some())
    }

    /// Lee un archivo completo (archivos chicos).
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.open_read(path)?.read_to_end(&mut out)?;
        Ok(out)
    }

    /// Escribe un archivo completo.
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let mut w = self.create(path)?;
        w.write_all(data)?;
        w.flush()
    }

    /// Crea la carpeta y las que falten por encima.
    fn create_dir_all(&self, path: &str) -> io::Result<()> {
        match self.stat(path)? {
            Some(e) if e.is_dir => return Ok(()),
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "existe y no es una carpeta",
                ));
            }
            None => {}
        }
        if let Some(parent) = self.parent(path)
            && parent != path
        {
            self.create_dir_all(&parent)?;
        }
        self.create_dir(path)
    }

    /// Borra un archivo o una carpeta con todo su contenido.
    fn remove_all(&self, path: &str) -> io::Result<()> {
        match self.stat(path)? {
            None => Ok(()),
            Some(e) if !e.is_dir => self.remove_file(path),
            Some(_) => {
                loop {
                    let listing = self.list(path)?;
                    for child in &listing.entries {
                        self.remove_all(&child.path)?;
                    }
                    if !listing.truncated {
                        break;
                    }
                }
                self.remove_dir(path)
            }
        }
    }
}

/// El disco local (incluidas las rutas de WSL `\\wsl$\…`, que Windows expone como locales).
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFs;

impl FileSystem for LocalFs {
    fn style(&self) -> PathStyle {
        PathStyle::Native
    }

    fn remote_label(&self) -> Option<String> {
        None
    }

    fn home(&self) -> io::Result<String> {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "sin carpeta personal"))
    }

    fn list(&self, dir: &str) -> io::Result<Listing> {
        list_dir(Path::new(dir))
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileEntry>> {
        match std::fs::metadata(path) {
            Ok(meta) => Ok(Some(FileEntry::from_metadata(
                self.file_name(path).unwrap_or_else(|| path.to_string()),
                path.to_string(),
                &meta,
            ))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn open_read(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(std::fs::File::open(path)?))
    }

    fn create(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(std::fs::File::create(path)?))
    }

    fn create_dir(&self, path: &str) -> io::Result<()> {
        std::fs::create_dir(path)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &str) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir(&self, path: &str) -> io::Result<()> {
        std::fs::remove_dir(path)
    }

    fn remove_all(&self, path: &str) -> io::Result<()> {
        match std::fs::metadata(path) {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(path),
            Ok(_) => std::fs::remove_file(path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("kuntur-fs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn local_crea_lee_renombra_y_borra() {
        let fs = LocalFs;
        let base = tmp("local");
        let sub = fs.join(&base, "a");
        let deep = fs.join(&fs.join(&sub, "b"), "c");
        fs.create_dir_all(&deep).unwrap();
        assert!(fs.stat(&deep).unwrap().unwrap().is_dir);
        let file = fs.join(&deep, "hola.txt");
        fs.write(&file, b"hola").unwrap();
        assert_eq!(fs.read(&file).unwrap(), b"hola");
        let st = fs.stat(&file).unwrap().unwrap();
        assert_eq!((st.name.as_str(), st.size, st.is_dir), ("hola.txt", 4, false));
        let moved = fs.join(&deep, "chao.txt");
        fs.rename(&file, &moved).unwrap();
        assert!(!fs.exists(&file).unwrap());
        assert_eq!(fs.list(&deep).unwrap().entries[0].name, "chao.txt");
        assert!(fs.remove_dir(&sub).is_err(), "no borra una carpeta con contenido");
        fs.remove_all(&sub).unwrap();
        assert!(fs.stat(&sub).unwrap().is_none());
        assert!(fs.remote_label().is_none());
        std::fs::remove_dir_all(&base).unwrap();
    }
}

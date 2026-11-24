//! Sistema de archivos POSIX en memoria: pruebas de la cola de transferencias y árbol "remoto"
//! **sintético** de las pruebas de humo (nunca datos reales).

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::sync::{Arc, Mutex};

use crate::fs::FileSystem;
use crate::path::{PathStyle, posix_parent};
use crate::{FileEntry, Listing, MAX_ENTRIES, sort_entries};

#[derive(Clone, Debug)]
enum Node {
    Dir,
    File { data: Vec<u8>, modified: i64 },
}

/// Árbol en memoria con rutas POSIX absolutas. Clonar comparte el mismo árbol.
#[derive(Clone)]
pub struct MemoryFs {
    nodes: Arc<Mutex<BTreeMap<String, Node>>>,
    label: Option<String>,
    home: String,
}

fn normalize(path: &str) -> String {
    let t = path.trim_end_matches('/');
    if t.is_empty() { "/".into() } else { t.to_string() }
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("no existe: {path}"))
}

impl MemoryFs {
    /// Árbol vacío (solo `/`). `label`: `Some("usuario@host")` para que se vea como remoto.
    pub fn new(label: Option<&str>, home: &str) -> Self {
        let fs = Self {
            nodes: Arc::new(Mutex::new(BTreeMap::from([("/".to_string(), Node::Dir)]))),
            label: label.map(String::from),
            home: normalize(home),
        };
        let _ = fs.create_dir_all(home);
        fs
    }

    /// Agrega un archivo (y sus carpetas) con fecha `modified` (segundos Unix).
    pub fn add_file(&self, path: &str, data: &[u8], modified: i64) {
        if let Some(parent) = posix_parent(path) {
            let _ = self.create_dir_all(&parent);
        }
        self.nodes.lock().unwrap().insert(
            normalize(path),
            Node::File {
                data: data.to_vec(),
                modified,
            },
        );
    }

    fn entry(path: &str, node: &Node) -> FileEntry {
        let name = PathStyle::Posix.file_name(path).unwrap_or_else(|| "/".into());
        match node {
            Node::Dir => FileEntry {
                name,
                path: path.to_string(),
                is_dir: true,
                size: 0,
                modified: None,
            },
            Node::File { data, modified } => FileEntry {
                name,
                path: path.to_string(),
                is_dir: false,
                size: data.len() as u64,
                modified: Some(*modified),
            },
        }
    }

    fn require_parent_dir(&self, path: &str) -> io::Result<()> {
        let parent = posix_parent(path).ok_or_else(|| not_found(path))?;
        match self.nodes.lock().unwrap().get(&parent) {
            Some(Node::Dir) => Ok(()),
            _ => Err(not_found(&parent)),
        }
    }
}

/// Escritor que confirma cada bloque en el árbol.
struct MemWriter {
    fs: MemoryFs,
    path: String,
}

impl Write for MemWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut nodes = self.fs.nodes.lock().unwrap();
        match nodes.get_mut(&self.path) {
            Some(Node::File { data, .. }) => data.extend_from_slice(buf),
            _ => return Err(not_found(&self.path)),
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl FileSystem for MemoryFs {
    fn style(&self) -> PathStyle {
        PathStyle::Posix
    }

    fn remote_label(&self) -> Option<String> {
        self.label.clone()
    }

    fn home(&self) -> io::Result<String> {
        Ok(self.home.clone())
    }

    fn list(&self, dir: &str) -> io::Result<Listing> {
        let dir = normalize(dir);
        let nodes = self.nodes.lock().unwrap();
        match nodes.get(&dir) {
            Some(Node::Dir) => {}
            Some(_) => return Err(io::Error::other("no es una carpeta")),
            None => return Err(not_found(&dir)),
        }
        let mut entries = Vec::new();
        let mut truncated = false;
        for (path, node) in nodes.iter() {
            if path != &dir && posix_parent(path).as_deref() == Some(dir.as_str()) {
                if entries.len() >= MAX_ENTRIES {
                    truncated = true;
                    break;
                }
                entries.push(Self::entry(path, node));
            }
        }
        sort_entries(&mut entries);
        Ok(Listing { entries, truncated })
    }

    fn stat(&self, path: &str) -> io::Result<Option<FileEntry>> {
        let path = normalize(path);
        Ok(self.nodes.lock().unwrap().get(&path).map(|n| Self::entry(&path, n)))
    }

    fn open_read(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
        match self.nodes.lock().unwrap().get(&normalize(path)) {
            Some(Node::File { data, .. }) => Ok(Box::new(Cursor::new(data.clone()))),
            Some(Node::Dir) => Err(io::Error::other("es una carpeta")),
            None => Err(not_found(path)),
        }
    }

    fn create(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        let path = normalize(path);
        self.require_parent_dir(&path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(Node::Dir) = nodes.get(&path) {
            return Err(io::Error::other("es una carpeta"));
        }
        nodes.insert(
            path.clone(),
            Node::File {
                data: Vec::new(),
                modified: 0,
            },
        );
        Ok(Box::new(MemWriter { fs: self.clone(), path }))
    }

    fn create_dir(&self, path: &str) -> io::Result<()> {
        let path = normalize(path);
        if path != "/" {
            self.require_parent_dir(&path)?;
        }
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "ya existe"));
        }
        nodes.insert(path, Node::Dir);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let (from, to) = (normalize(from), normalize(to));
        self.require_parent_dir(&to)?;
        let mut nodes = self.nodes.lock().unwrap();
        if !nodes.contains_key(&from) {
            return Err(not_found(&from));
        }
        let prefix = format!("{from}/");
        let moved: Vec<String> = nodes
            .keys()
            .filter(|k| **k == from || k.starts_with(&prefix))
            .cloned()
            .collect();
        for key in moved {
            let node = nodes.remove(&key).unwrap();
            nodes.insert(format!("{to}{}", &key[from.len()..]), node);
        }
        Ok(())
    }

    fn remove_file(&self, path: &str) -> io::Result<()> {
        let path = normalize(path);
        let mut nodes = self.nodes.lock().unwrap();
        match nodes.get(&path) {
            Some(Node::File { .. }) => {
                nodes.remove(&path);
                Ok(())
            }
            Some(Node::Dir) => Err(io::Error::other("es una carpeta")),
            None => Err(not_found(&path)),
        }
    }

    fn remove_dir(&self, path: &str) -> io::Result<()> {
        let path = normalize(path);
        let mut nodes = self.nodes.lock().unwrap();
        let prefix = format!("{path}/");
        if nodes.keys().any(|k| k.starts_with(&prefix)) {
            return Err(io::Error::other("la carpeta no está vacía"));
        }
        match nodes.get(&path) {
            Some(Node::Dir) => {
                nodes.remove(&path);
                Ok(())
            }
            Some(_) => Err(io::Error::other("no es una carpeta")),
            None => Err(not_found(&path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbol_en_memoria() {
        let fs = MemoryFs::new(Some("demo@servidor"), "/home/demo");
        assert_eq!(fs.home().unwrap(), "/home/demo");
        fs.add_file("/home/demo/web/index.html", b"<h1>", 10);
        fs.create_dir("/home/demo/datos").unwrap();
        let l = fs.list("/home/demo").unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["datos", "web"]);
        assert_eq!(fs.read("/home/demo/web/index.html").unwrap(), b"<h1>");
        fs.write("/home/demo/datos/a.csv", b"1,2").unwrap();
        assert_eq!(fs.stat("/home/demo/datos/a.csv").unwrap().unwrap().size, 3);
        assert!(fs.create("/no/existe/x").is_err());
        fs.rename("/home/demo/web", "/home/demo/sitio").unwrap();
        assert!(fs.exists("/home/demo/sitio/index.html").unwrap());
        assert!(fs.remove_dir("/home/demo/sitio").is_err());
        fs.remove_all("/home/demo/sitio").unwrap();
        assert!(!fs.exists("/home/demo/sitio").unwrap());
        assert_eq!(fs.remote_label().as_deref(), Some("demo@servidor"));
    }
}

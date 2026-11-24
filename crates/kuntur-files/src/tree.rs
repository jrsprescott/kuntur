//! Árbol del panel "Archivos" (ajustes v0.2), como el Explorer de VS Code: una carpeta raíz,
//! carpetas desplegadas con su contenido leído **perezosamente** (solo al desplegarlas) y la lista
//! "aplanada" de filas visibles, con su profundidad, para dibujarla en una lista virtualizada.
//!
//! Lógica pura: no lee el disco. La app pide [`Tree::dirs_to_read`], lee esas carpetas fuera del
//! hilo de la UI con el [`crate::FileSystem`] de la pestaña y entrega cada resultado con
//! [`Tree::set_listing`].

use std::collections::{BTreeSet, HashMap};

use crate::path::PathStyle;
use crate::{FileEntry, Listing};

/// Contenido conocido de una carpeta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirState {
    Loaded(Listing),
    Error(String),
}

/// Una fila visible del árbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub entry: FileEntry,
    /// 0 = hijos directos de la raíz.
    pub depth: usize,
    /// Carpeta desplegada.
    pub expanded: bool,
    /// Carpeta desplegada cuyo contenido todavía no llegó.
    pub loading: bool,
    /// Carpeta desplegada que no se pudo leer (el motivo).
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Tree {
    style: PathStyle,
    root: Option<String>,
    expanded: BTreeSet<String>,
    dirs: HashMap<String, DirState>,
}

impl Tree {
    pub fn new(style: PathStyle) -> Self {
        Self {
            style,
            root: None,
            expanded: BTreeSet::new(),
            dirs: HashMap::new(),
        }
    }

    pub fn style(&self) -> PathStyle {
        self.style
    }

    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    /// Cambia la raíz. Si es otra, se olvida todo lo desplegado y leído.
    pub fn set_root(&mut self, root: Option<String>) -> bool {
        if self.root == root {
            return false;
        }
        self.root = root;
        self.expanded.clear();
        self.dirs.clear();
        true
    }

    /// Contenido de la raíz (`None` mientras no se leyó).
    pub fn root_state(&self) -> Option<&DirState> {
        self.dirs.get(self.root.as_deref()?)
    }

    pub fn is_expanded(&self, dir: &str) -> bool {
        self.expanded.contains(dir)
    }

    /// Despliega una carpeta. `true` si hay que leerla (todavía no se conoce su contenido).
    pub fn expand(&mut self, dir: &str) -> bool {
        self.expanded.insert(dir.to_string());
        !self.dirs.contains_key(dir)
    }

    /// Pliega una carpeta (lo desplegado dentro se recuerda, como en VS Code).
    pub fn collapse(&mut self, dir: &str) {
        self.expanded.remove(dir);
    }

    /// Alterna desplegada / plegada. `true` si hay que leerla.
    pub fn toggle(&mut self, dir: &str) -> bool {
        if self.is_expanded(dir) {
            self.collapse(dir);
            false
        } else {
            self.expand(dir)
        }
    }

    /// Carpetas que hay que leer para dibujar el árbol: la raíz y las desplegadas que se ven (las
    /// de dentro de una carpeta plegada no se leen). Con `refresh` incluye también las ya leídas
    /// (relectura periódica); si no, solo las que faltan.
    pub fn dirs_to_read(&self, refresh: bool) -> Vec<String> {
        let Some(root) = self.root.clone() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            match self.dirs.get(&dir) {
                Some(DirState::Loaded(listing)) => {
                    if refresh {
                        out.push(dir.clone());
                    }
                    for e in listing.entries.iter().rev() {
                        if e.is_dir && self.expanded.contains(&e.path) {
                            stack.push(e.path.clone());
                        }
                    }
                }
                Some(DirState::Error(_)) => {
                    if refresh {
                        out.push(dir.clone());
                    }
                }
                None => out.push(dir.clone()),
            }
        }
        out
    }

    /// Guarda el contenido leído de `dir`. `true` si cambió lo que se ve.
    pub fn set_listing(&mut self, dir: &str, result: Result<Listing, String>) -> bool {
        let state = match result {
            Ok(l) => DirState::Loaded(l),
            Err(e) => DirState::Error(e),
        };
        if self.dirs.get(dir) == Some(&state) {
            return false;
        }
        self.dirs.insert(dir.to_string(), state);
        true
    }

    /// Olvida el contenido de `dir` (y de lo que tenga dentro) para volver a leerlo; lo desplegado se
    /// mantiene. Tras crear, renombrar, pegar o borrar.
    pub fn invalidate(&mut self, dir: &str) {
        let style = self.style;
        self.dirs.retain(|k, _| !style.is_within(dir, k));
    }

    /// Una ruta desapareció (se borró o renombró): se quita del estado.
    pub fn forget(&mut self, path: &str) {
        let style = self.style;
        self.expanded.retain(|k| !style.is_within(path, k));
        self.dirs.retain(|k, _| !style.is_within(path, k));
    }

    /// Filas visibles, en orden: cada carpeta desplegada seguida de su contenido.
    pub fn rows(&self) -> Vec<TreeRow> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            self.push_rows(root, 0, &mut out);
        }
        out
    }

    fn push_rows(&self, dir: &str, depth: usize, out: &mut Vec<TreeRow>) {
        let Some(DirState::Loaded(listing)) = self.dirs.get(dir) else {
            return;
        };
        for e in &listing.entries {
            let expanded = e.is_dir && self.expanded.contains(&e.path);
            let state = expanded.then(|| self.dirs.get(&e.path)).flatten();
            out.push(TreeRow {
                entry: e.clone(),
                depth,
                expanded,
                loading: expanded && state.is_none(),
                error: match state {
                    Some(DirState::Error(err)) => Some(err.clone()),
                    _ => None,
                },
            });
            if expanded {
                self.push_rows(&e.path, depth + 1, out);
            }
        }
    }
}

/// Índice de la fila "madre" (la carpeta que contiene a la fila `index`), para ← en el teclado.
pub fn parent_row(rows: &[TreeRow], index: usize) -> Option<usize> {
    let depth = rows.get(index)?.depth;
    if depth == 0 {
        return None;
    }
    rows[..index].iter().rposition(|r| r.depth == depth - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::FileSystem;
    use crate::memory::MemoryFs;

    /// Árbol sintético: /p/{src/{main.rs, util/{a.rs}}, docs/, README.md, Cargo.toml}.
    fn fs() -> MemoryFs {
        let fs = MemoryFs::new(None, "/p");
        fs.add_file("/p/src/main.rs", b"fn main() {}", 1);
        fs.add_file("/p/src/util/a.rs", b"", 1);
        fs.add_file("/p/README.md", b"# hola", 1);
        fs.add_file("/p/Cargo.toml", b"", 1);
        fs.create_dir("/p/docs").unwrap();
        fs
    }

    /// Lee lo que el árbol pide, como hace la app.
    fn load(tree: &mut Tree, fs: &MemoryFs, refresh: bool) {
        for dir in tree.dirs_to_read(refresh) {
            tree.set_listing(&dir, fs.list(&dir).map_err(|e| e.to_string()));
        }
    }

    fn names(tree: &Tree) -> Vec<String> {
        tree.rows()
            .iter()
            .map(|r| {
                format!(
                    "{}{}{}",
                    "  ".repeat(r.depth),
                    r.entry.name,
                    if r.entry.is_dir { "/" } else { "" }
                )
            })
            .collect()
    }

    #[test]
    fn solo_la_raiz_al_principio_y_carpetas_primero() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        assert!(tree.rows().is_empty());
        tree.set_root(Some("/p".into()));
        assert_eq!(tree.dirs_to_read(false), ["/p"]);
        load(&mut tree, &fs, false);
        assert_eq!(names(&tree), ["docs/", "src/", "Cargo.toml", "README.md"]);
        assert!(tree.dirs_to_read(false).is_empty(), "nada más que leer");
    }

    #[test]
    fn desplegar_lee_perezosamente_y_aplana_con_profundidad() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        load(&mut tree, &fs, false);
        assert!(tree.toggle("/p/src"), "hay que leerla");
        // Mientras no llega, la fila dice "cargando" y no tiene hijos.
        let row = tree.rows().into_iter().find(|r| r.entry.path == "/p/src").unwrap();
        assert!(row.expanded && row.loading);
        assert_eq!(tree.dirs_to_read(false), ["/p/src"]);
        load(&mut tree, &fs, false);
        tree.expand("/p/src/util");
        load(&mut tree, &fs, false);
        assert_eq!(
            names(&tree),
            [
                "docs/",
                "src/",
                "  util/",
                "    a.rs",
                "  main.rs",
                "Cargo.toml",
                "README.md"
            ]
        );
        // Plegar oculta todo lo de dentro sin olvidarlo; al volver a desplegar no se relee.
        assert!(!tree.toggle("/p/src"));
        assert_eq!(names(&tree), ["docs/", "src/", "Cargo.toml", "README.md"]);
        assert!(!tree.toggle("/p/src"));
        assert_eq!(names(&tree).len(), 7);
    }

    #[test]
    fn la_relectura_periodica_solo_toca_lo_visible() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        load(&mut tree, &fs, false);
        tree.expand("/p/src");
        load(&mut tree, &fs, false);
        tree.expand("/p/src/util");
        load(&mut tree, &fs, false);
        tree.collapse("/p/src");
        // util sigue desplegada, pero dentro de una plegada: no se relee.
        assert_eq!(tree.dirs_to_read(true), ["/p"]);
        // Un archivo nuevo aparece al releer, y releer lo mismo no "cambia" nada.
        fs.add_file("/p/nuevo.txt", b"", 2);
        let mut changed = false;
        for dir in tree.dirs_to_read(true) {
            changed |= tree.set_listing(&dir, fs.list(&dir).map_err(|e| e.to_string()));
        }
        assert!(changed);
        assert!(names(&tree).contains(&"nuevo.txt".to_string()));
        assert!(!tree.set_listing("/p", fs.list("/p").map_err(|e| e.to_string())));
    }

    #[test]
    fn otra_raiz_olvida_todo() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        load(&mut tree, &fs, false);
        tree.expand("/p/src");
        assert!(!tree.set_root(Some("/p".into())), "misma raíz: nada cambia");
        assert!(tree.is_expanded("/p/src"));
        assert!(tree.set_root(Some("/p/src".into())));
        assert!(!tree.is_expanded("/p/src"));
        assert!(tree.rows().is_empty());
        assert_eq!(tree.dirs_to_read(false), ["/p/src"]);
    }

    #[test]
    fn carpeta_que_no_se_puede_leer() {
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        let mut l = Listing::default();
        l.entries.push(FileEntry {
            name: "privada".into(),
            path: "/p/privada".into(),
            is_dir: true,
            size: 0,
            modified: None,
        });
        tree.set_listing("/p", Ok(l));
        tree.expand("/p/privada");
        tree.set_listing("/p/privada", Err("permiso denegado".into()));
        let row = &tree.rows()[0];
        assert!(row.expanded && !row.loading);
        assert_eq!(row.error.as_deref(), Some("permiso denegado"));
    }

    #[test]
    fn invalidar_y_olvidar_tras_cambios() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        load(&mut tree, &fs, false);
        tree.expand("/p/src");
        load(&mut tree, &fs, false);
        tree.expand("/p/src/util");
        load(&mut tree, &fs, false);
        // Crear algo en src: se relee src (y lo de dentro), lo desplegado se mantiene.
        fs.add_file("/p/src/lib.rs", b"", 3);
        tree.invalidate("/p/src");
        assert_eq!(tree.dirs_to_read(false), ["/p/src"]);
        load(&mut tree, &fs, false);
        load(&mut tree, &fs, false);
        assert!(names(&tree).contains(&"  lib.rs".to_string()));
        assert!(tree.is_expanded("/p/src/util"));
        // Borrar src: se olvida su estado; `/p/srcx` (mismo prefijo) no se toca.
        tree.expand("/p/srcx");
        tree.forget("/p/src");
        assert!(!tree.is_expanded("/p/src") && !tree.is_expanded("/p/src/util"));
        assert!(tree.is_expanded("/p/srcx"));
    }

    #[test]
    fn fila_madre_para_la_flecha_izquierda() {
        let fs = fs();
        let mut tree = Tree::new(PathStyle::Posix);
        tree.set_root(Some("/p".into()));
        load(&mut tree, &fs, false);
        tree.expand("/p/src");
        load(&mut tree, &fs, false);
        tree.expand("/p/src/util");
        load(&mut tree, &fs, false);
        let rows = tree.rows();
        // ["docs/", "src/", "  util/", "    a.rs", "  main.rs", ...]
        assert_eq!(parent_row(&rows, 3), Some(2));
        assert_eq!(parent_row(&rows, 4), Some(1));
        assert_eq!(parent_row(&rows, 1), None);
        assert_eq!(parent_row(&rows, 99), None);
    }

    #[test]
    fn rutas_de_windows() {
        let mut tree = Tree::new(PathStyle::Native);
        let sep = std::path::MAIN_SEPARATOR;
        let root = format!("raiz{sep}p");
        let src = format!("{root}{sep}src");
        tree.set_root(Some(root.clone()));
        tree.expand(&src);
        tree.expand(&format!("{src}{sep}util"));
        tree.forget(&src);
        assert!(!tree.is_expanded(&src));
        assert!(!tree.is_expanded(&format!("{src}{sep}util")));
    }
}

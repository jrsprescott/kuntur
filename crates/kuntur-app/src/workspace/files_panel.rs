//! Contenido "Archivos" del panel izquierdo: la carpeta actual de la sesión **activa**, local o
//! remota por SFTP (F7), **nunca mezcladas**: el encabezado muestra siempre la ruta y la etiqueta
//! "local" o "remoto · usuario@host".
//!
//! Ajustes v0.2: **árbol** como el Explorer de VS Code (chevron `›` que gira al desplegar, sangría
//! con guías, carga perezosa por carpeta) y **menú contextual** con las opciones de VS Code (ver
//! `crate::file_menu`), en español o inglés.
//!
//! - Clic en carpeta despliega/pliega; doble clic en un archivo local lo abre con su programa.
//! - Teclado con el árbol enfocado: ↑/↓ eligen, → despliega o entra, ← pliega o sube, Enter abre
//!   (vista previa) o despliega, F2 cambia el nombre, Supr elimina, Ctrl+X/C/V cortan, copian y
//!   pegan, Shift+Alt+C copia la ruta, Ctrl+K Ctrl+Shift+C la ruta relativa, Shift+Alt+R muestra
//!   en el gestor de archivos, Esc vuelve a la terminal.
//! - Soltar archivos del Explorador sobre una carpeta (o sobre la lista) = subir por SFTP a esa
//!   carpeta (remoto) o copiarlos (local). La cola está en `transfers_view.rs`.
//! - Remoto: zona "para el agente" que sube a `/tmp/kuntur-<sesión>/` y escribe las rutas en la
//!   terminal (§7).
//!
//! Toda lectura y escritura corre fuera del hilo de la UI.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use kuntur_agents::ShellFlavor;
use kuntur_files::{
    DirState, Direction, FileEntry, FileSystem, JobId, Listing, LocalFs, MemoryFs, PathStyle, Transfers, Tree, TreeRow,
    agent_dir, conflict_name, format_modified, format_size, location_label, parent_row, posix_quote_all,
};
use kuntur_ssh::RemoteSession;

use super::*;
use crate::file_menu::{
    MenuAction, MenuContext, MenuEntry, MenuItem, build_menu, chat_reference, python_venv_line, share_menu,
    validate_name,
};
use crate::i18n::{T, t};

const ROW: f32 = 24.0;
/// Sangría por nivel del árbol.
const INDENT: f32 = 12.0;
/// Margen izquierdo de las filas (y de la primera guía).
const ROW_PAD: f32 = 6.0;
const MENU_W: f32 = 310.0;
const MENU_ITEM_H: f32 = 26.0;
const MENU_SEP_H: f32 = 9.0;
const MENU_PAD: f32 = 4.0;
/// Imágenes que se abren a la vez con "Abrir en vista previa de imágenes".
const MAX_GALLERY: usize = 12;
const IMAGE_EXTS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"];

/// De dónde lee el árbol la pestaña activa.
#[derive(Clone)]
enum FilesSource {
    Local {
        cwd: Option<String>,
    },
    Remote {
        session: Arc<RemoteSession>,
        cwd: Option<String>,
    },
}

/// Menú contextual abierto sobre una entrada.
struct FileMenu {
    entry: FileEntry,
    position: Point<Pixels>,
    /// Submenú "Compartir ▸" abierto.
    share_open: bool,
}

/// Campo en línea del árbol: nuevo archivo/carpeta o cambio de nombre.
#[derive(Clone)]
enum EditKind {
    New { parent: String, is_dir: bool },
    Rename { entry: FileEntry },
}

#[derive(Clone)]
struct InlineEdit {
    kind: EditKind,
    text: String,
    error: Option<T>,
}

/// Archivos copiados o cortados en el árbol (de cualquier pestaña: pegar en otra local o remota
/// pasa por la cola de transferencias).
#[derive(Clone)]
struct FileClip {
    fs: Arc<dyn FileSystem>,
    remote: bool,
    entries: Vec<FileEntry>,
    cut: bool,
}

/// Cortar y pegar entre sistemas distintos: al terminar la copia se borra el original.
struct CutJob {
    job: JobId,
    fs: Arc<dyn FileSystem>,
    path: String,
}

/// Confirmación de "Eliminar" (local, a la Papelera) o "ADVERTENCIA: Eliminar esto" (remoto).
struct ConfirmDelete {
    entry: FileEntry,
    remote: bool,
}

/// Subidas "para el agente" de una pestaña: al terminar, sus rutas se escriben en la terminal.
struct AgentGroup {
    session: u64,
    dest_dir: String,
    jobs: Vec<JobId>,
}

/// Fila que se dibuja: una entrada del árbol o el campo para un nombre nuevo.
#[derive(Clone)]
enum ViewRow {
    Entry(TreeRow),
    NewInput { depth: usize, is_dir: bool },
}

pub(super) struct FilesState {
    /// Pestaña cuyo árbol se muestra: al cambiar de pestaña, todo el estado se reinicia.
    session: Option<u64>,
    /// ¿La pestaña es remota? (se fija junto con `session`).
    remote: bool,
    /// Sistema de archivos de la pestaña (local, SFTP o sintético en las pruebas de humo).
    fs: Option<Arc<dyn FileSystem>>,
    /// Remota sin SFTP: el motivo ("sin conexión con el servidor", "conectando…").
    fs_error: Option<String>,
    /// Raíz del árbol (puede ser otra si el usuario subió o bajó la raíz).
    dir: Option<String>,
    /// Carpeta de la sesión la última vez que se sincronizó: si la sesión cambia de carpeta, el
    /// panel la sigue.
    follow: Option<String>,
    tree: Tree,
    selected: Option<String>,
    /// Lectura en curso (se cancela al soltarla).
    task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
    /// Foco del árbol (atajos de teclado).
    pub(super) focus: FocusHandle,
    menu: Option<FileMenu>,
    edit: Option<InlineEdit>,
    clip: Option<FileClip>,
    /// Se pulsó Ctrl+K (primera mitad de Ctrl+K Ctrl+Shift+C).
    chord_k: bool,
    confirm_delete: Option<ConfirmDelete>,
    /// Aviso corto al pie (ruta copiada, error al borrar…).
    pub(super) notice: Option<String>,
    /// Cola de transferencias (todas las pestañas).
    pub(super) transfers: Transfers,
    /// Repinta mientras haya transferencias en curso.
    poll: Option<Task<()>>,
    agent_groups: Vec<AgentGroup>,
    cut_jobs: Vec<CutJob>,
    /// Prueba de humo: árbol remoto **sintético** (nunca un servidor real).
    smoke_fs: Option<Arc<dyn FileSystem>>,
}

impl FilesState {
    pub(super) fn new(cx: &mut Context<Workspace>) -> Self {
        Self {
            session: None,
            remote: false,
            fs: None,
            fs_error: None,
            dir: None,
            follow: None,
            tree: Tree::new(PathStyle::Native),
            selected: None,
            task: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            menu: None,
            edit: None,
            clip: None,
            chord_k: false,
            confirm_delete: None,
            notice: None,
            transfers: Transfers::default(),
            poll: None,
            agent_groups: Vec::new(),
            cut_jobs: Vec::new(),
            smoke_fs: None,
        }
    }
}

/// Ícono según el tipo de archivo.
fn file_icon(entry: &FileEntry, expanded: bool) -> IconName {
    if entry.is_dir {
        return if expanded {
            IconName::FolderOpen
        } else {
            IconName::Folder
        };
    }
    let ext = entry
        .name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => IconName::FileImage,
        "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "xz" | "zst" | "bz2" => IconName::FileArchive,
        "md" | "txt" | "log" | "csv" | "pdf" | "doc" | "docx" | "rtf" => IconName::FileText,
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "kt" | "rb"
        | "php" | "sh" | "ps1" | "bat" | "cmd" | "toml" | "yaml" | "yml" | "json" | "html" | "css" | "scss" | "sql"
        | "lua" | "swift" | "vue" | "svelte" | "xml" => IconName::FileCode,
        _ => IconName::File,
    }
}

/// Salida de `git -C dir …` (sin abrir una consola en Windows). `None` si falla.
fn git_output(dir: &str, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// "Mostrar en el gestor de archivos": el Explorador con el elemento elegido (Windows), Finder
/// (Mac) o la carpeta contenedora (Linux).
fn reveal_in_file_manager(path: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{path}\""))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new(path));
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

impl Workspace {
    /// Fuente del árbol de la pestaña activa.
    fn files_source(&self, cx: &App) -> Option<(u64, FilesSource)> {
        let entry = self.sessions.active()?;
        let source = match &entry.value.remote {
            Some(r) => FilesSource::Remote {
                session: r.session.clone(),
                cwd: r.main.cwd(),
            },
            None => FilesSource::Local {
                cwd: self.active_cwd(cx).map(|p| p.to_string_lossy().into_owned()),
            },
        };
        Some((entry.id, source))
    }

    /// Sigue la carpeta de la sesión activa y vuelve a leer lo que se ve del árbol. Solo trabaja con
    /// el panel en "Archivos" y visible; `force` relee aunque haya una lectura en curso.
    pub(super) fn sync_files(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.panel != PanelView::Files || self.settings.sidebar != SidebarMode::Full {
            return;
        }
        let Some((id, source)) = self.files_source(cx) else {
            return;
        };
        let (remote, cwd) = match &source {
            FilesSource::Local { cwd } => (false, cwd.clone()),
            FilesSource::Remote { cwd, .. } => (true, cwd.clone()),
        };
        if self.files.session != Some(id) {
            // Otra pestaña: nada del árbol anterior sobrevive (nunca mezclar local con remoto).
            self.files.session = Some(id);
            self.files.remote = remote;
            self.files.fs = (!remote).then(|| Arc::new(LocalFs) as Arc<dyn FileSystem>);
            self.files.fs_error = None;
            self.files.dir = None;
            self.files.follow = None;
            self.files.tree = Tree::new(if remote { PathStyle::Posix } else { PathStyle::Native });
            self.files.selected = None;
            self.files.menu = None;
            self.files.edit = None;
            self.files.confirm_delete = None;
            self.files.task = None;
        }
        if cwd.is_some() && cwd != self.files.follow {
            self.files.follow = cwd.clone();
            self.files.dir = cwd.clone();
            self.files.tree.set_root(cwd);
            self.files.selected = None;
            self.files.edit = None;
        }
        // Mientras hay una lectura en curso no se lanza otra (salvo que falte la raíz).
        if force || self.files.task.is_none() || self.files.tree.root_state().is_none() {
            self.load_files(true, cx);
        }
    }

    /// Lee las carpetas que el árbol necesita: con `refresh`, todas las visibles; si no, solo las
    /// que faltan (recién desplegadas).
    fn load_files(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some((id, source)) = self.files_source(cx) else {
            return;
        };
        if self.files.session != Some(id) {
            return;
        }
        let root = self.files.dir.clone();
        // Local: sin carpeta conocida no hay nada que leer.
        let remote = match source {
            FilesSource::Local { .. } if root.is_none() => {
                self.files.task = None;
                return;
            }
            FilesSource::Local { .. } => None,
            FilesSource::Remote { session, .. } => Some(session),
        };
        let dirs = if root.is_some() {
            self.files.tree.dirs_to_read(refresh)
        } else {
            Vec::new()
        };
        if root.is_some() && dirs.is_empty() {
            return;
        }
        let smoke = self.files.smoke_fs.clone();
        let requested = root.clone();
        self.files.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let fs: Arc<dyn FileSystem> = match (remote, smoke) {
                        (None, _) => Arc::new(LocalFs),
                        (Some(_), Some(fake)) => fake,
                        (Some(session), None) => session.sftp().map_err(|e| e.to_string())?,
                    };
                    let (root, dirs) = match root {
                        Some(r) => (r, dirs),
                        None => {
                            let home = fs.home().map_err(|e| e.to_string())?;
                            (home.clone(), vec![home])
                        }
                    };
                    let listings: Vec<(String, Result<Listing, String>)> = dirs
                        .into_iter()
                        .map(|d| {
                            let l = fs.list(&d).map_err(|e| e.to_string());
                            (d, l)
                        })
                        .collect();
                    Ok::<_, String>((fs, root, listings))
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.files.task = None;
                if w.files.session != Some(id) || w.files.dir != requested {
                    return;
                }
                // Se relee en cada vuelta del monitor: solo se redibuja si cambió lo que se ve.
                let changed = match result {
                    Ok((fs, root, listings)) => {
                        let mut changed = w.files.fs_error.is_some() || w.files.dir.as_ref() != Some(&root);
                        w.files.fs = Some(fs);
                        w.files.fs_error = None;
                        w.files.dir = Some(root.clone());
                        changed |= w.files.tree.set_root(Some(root));
                        for (dir, listing) in listings {
                            changed |= w.files.tree.set_listing(&dir, listing);
                        }
                        changed
                    }
                    Err(e) => {
                        let changed = w.files.fs_error.as_ref() != Some(&e);
                        w.files.fs = None;
                        w.files.fs_error = Some(e);
                        changed
                    }
                };
                if changed {
                    cx.notify();
                }
            });
        }));
    }

    /// Cambia la raíz del árbol.
    fn open_dir(&mut self, dir: String, cx: &mut Context<Self>) {
        self.files.dir = Some(dir.clone());
        self.files.tree.set_root(Some(dir));
        self.files.selected = None;
        self.files.menu = None;
        self.files.edit = None;
        self.files.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.load_files(false, cx);
        cx.notify();
    }

    fn files_parent(&self) -> Option<String> {
        let fs = self.files.fs.as_ref()?;
        let dir = self.files.dir.as_deref()?;
        fs.parent(dir).filter(|p| p != dir)
    }

    fn files_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.files_parent() {
            self.open_dir(parent, cx);
        }
    }

    /// Pliega todas las carpetas (botón del encabezado).
    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        let root = self.files.dir.clone();
        let style = self.files.tree.style();
        let mut tree = Tree::new(style);
        tree.set_root(root.clone());
        // Se conserva el contenido leído de la raíz para no parpadear.
        if let (Some(r), Some(DirState::Loaded(l))) = (root.as_deref(), self.files.tree.root_state()) {
            tree.set_listing(r, Ok(l.clone()));
        }
        self.files.tree = tree;
        cx.notify();
    }

    /// Despliega o pliega una carpeta (y la lee si hace falta).
    fn toggle_dir(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.files.tree.toggle(path) {
            self.load_files(false, cx);
        }
        cx.notify();
    }

    /// Filas que se dibujan: el árbol y, si se está creando algo, el campo del nombre nuevo bajo su
    /// carpeta.
    fn view_rows(&self) -> Vec<ViewRow> {
        let mut out: Vec<ViewRow> = self.files.tree.rows().into_iter().map(ViewRow::Entry).collect();
        if let Some(InlineEdit {
            kind: EditKind::New { parent, is_dir },
            ..
        }) = &self.files.edit
        {
            let at = out
                .iter()
                .position(|r| matches!(r, ViewRow::Entry(t) if &t.entry.path == parent));
            let (index, depth) = match at {
                Some(i) => match &out[i] {
                    ViewRow::Entry(t) => (i + 1, t.depth + 1),
                    ViewRow::NewInput { .. } => (0, 0),
                },
                None => (0, 0),
            };
            out.insert(index, ViewRow::NewInput { depth, is_dir: *is_dir });
        }
        out
    }

    /// Entrada de la raíz (para las acciones sin nada elegido).
    fn root_entry(&self) -> Option<FileEntry> {
        let root = self.files.dir.clone()?;
        let name = self
            .files
            .fs
            .as_ref()
            .and_then(|f| f.file_name(&root))
            .unwrap_or_else(|| root.clone());
        Some(FileEntry {
            name,
            path: root,
            is_dir: true,
            size: 0,
            modified: None,
        })
    }

    /// La entrada elegida (o la raíz).
    fn selected_entry(&self) -> Option<FileEntry> {
        let sel = self.files.selected.as_deref();
        sel.and_then(|p| {
            self.files
                .tree
                .rows()
                .into_iter()
                .find(|r| r.entry.path == p)
                .map(|r| r.entry)
        })
        .or_else(|| self.root_entry())
    }

    fn on_row_click(&mut self, entry: &FileEntry, ev: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.files.focus, cx);
        self.files.menu = None;
        self.files.selected = Some(entry.path.clone());
        if entry.is_dir {
            self.toggle_dir(&entry.path, cx);
        } else if ev.click_count() >= 2 && !self.files.remote {
            cx.open_with_system(std::path::Path::new(&entry.path));
        }
        cx.notify();
    }

    fn open_file_menu(
        &mut self,
        entry: FileEntry,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.files.focus, cx);
        self.files.selected = Some(entry.path.clone());
        self.files.menu = Some(FileMenu {
            entry,
            position,
            share_open: false,
        });
        cx.notify();
    }

    fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        if self.files.menu.take().is_some() {
            cx.notify();
        }
    }

    fn files_notice(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.files.notice = Some(text.into());
        cx.notify();
    }

    // ── Teclado ─────────────────────────────────────────────────────────────

    /// Teclas con el árbol enfocado. `false` = no era para el árbol (sigue a los atajos globales).
    pub(super) fn on_files_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let key = ks.key.to_ascii_lowercase();
        if self.files.edit.is_some() {
            self.on_edit_key(&key, m, ks.key_char.as_deref(), cx);
            return true;
        }
        if self.files.confirm_delete.is_some() {
            match key.as_str() {
                "enter" => self.delete_confirmed(cx),
                "escape" => self.files.confirm_delete = None,
                _ => {}
            }
            cx.notify();
            return true;
        }
        if std::mem::take(&mut self.files.chord_k) && m.control && m.shift && key == "c" {
            if let Some(e) = self.selected_entry() {
                self.menu_action(MenuAction::CopyRelativePath, e, window, cx);
            }
            return true;
        }
        if m.control && !m.shift && !m.alt && key == "k" {
            self.files.chord_k = true;
            return true;
        }
        if key == "escape" && self.files.menu.is_some() {
            self.close_file_menu(cx);
            return true;
        }
        let rows = self.files.tree.rows();
        let index = self
            .files
            .selected
            .as_ref()
            .and_then(|p| rows.iter().position(|r| &r.entry.path == p));
        let action = match (m.control, m.shift, m.alt, key.as_str()) {
            (false, false, false, "up") => {
                let i = index.map_or(0, |i| i.saturating_sub(1));
                self.select_row(&rows, i);
                None
            }
            (false, false, false, "down") => {
                let i = index.map_or(0, |i| (i + 1).min(rows.len().saturating_sub(1)));
                self.select_row(&rows, i);
                None
            }
            (false, false, false, "right") => {
                if let Some(i) = index {
                    let r = &rows[i];
                    if r.entry.is_dir && !r.expanded {
                        self.toggle_dir(&r.entry.path.clone(), cx);
                    } else if r.entry.is_dir && rows.get(i + 1).is_some_and(|n| n.depth > r.depth) {
                        self.select_row(&rows, i + 1);
                    }
                }
                None
            }
            (false, false, false, "left") => {
                if let Some(i) = index {
                    let r = &rows[i];
                    if r.entry.is_dir && r.expanded {
                        self.files.tree.collapse(&r.entry.path);
                    } else if let Some(p) = parent_row(&rows, i) {
                        self.select_row(&rows, p);
                    }
                }
                None
            }
            (false, false, false, "enter") => match index.map(|i| rows[i].entry.clone()) {
                Some(e) if e.is_dir => {
                    self.toggle_dir(&e.path, cx);
                    None
                }
                Some(e) if !self.files.remote => {
                    self.open_preview(PathBuf::from(&e.path), None, window, cx);
                    None
                }
                _ => None,
            },
            (false, false, false, "f2") => self.selected_entry().map(|e| (MenuAction::Rename, e)),
            (false, false, false, "delete") => self.selected_entry().map(|e| {
                let a = if self.files.remote {
                    MenuAction::WarningDelete
                } else {
                    MenuAction::Delete
                };
                (a, e)
            }),
            (true, false, false, "c") => self.selected_entry().map(|e| (MenuAction::Copy, e)),
            (true, false, false, "x") => self.selected_entry().map(|e| (MenuAction::Cut, e)),
            (true, false, false, "v") => self.selected_entry().map(|e| (MenuAction::Paste, e)),
            (false, true, true, "c") => self.selected_entry().map(|e| (MenuAction::CopyPath, e)),
            (false, true, true, "r") => self.selected_entry().map(|e| (MenuAction::Reveal, e)),
            (false, false, false, "escape") => {
                self.focus_active(window, cx);
                None
            }
            _ => return false,
        };
        if let Some((action, entry)) = action {
            // La raíz no se renombra, ni se elimina, ni se corta desde el teclado.
            let is_root = self.files.dir.as_deref() == Some(entry.path.as_str());
            let protected = matches!(
                action,
                MenuAction::Rename | MenuAction::Delete | MenuAction::WarningDelete | MenuAction::Cut
            );
            if !(is_root && protected) {
                self.menu_action(action, entry, window, cx);
            }
        }
        cx.notify();
        true
    }

    fn select_row(&mut self, rows: &[TreeRow], index: usize) {
        if let Some(r) = rows.get(index) {
            self.files.selected = Some(r.entry.path.clone());
            self.files.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    /// Teclado del campo en línea (nombre nuevo o cambio de nombre).
    fn on_edit_key(&mut self, key: &str, m: Modifiers, key_char: Option<&str>, cx: &mut Context<Self>) {
        let Some(edit) = self.files.edit.as_mut() else {
            return;
        };
        match key {
            "escape" => self.files.edit = None,
            "enter" => self.commit_edit(cx),
            "backspace" if m.control => {
                edit.text.clear();
                edit.error = None;
            }
            "backspace" => {
                edit.text.pop();
                edit.error = None;
            }
            "v" if m.control && !m.alt => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    let line = text.lines().next().unwrap_or("").to_string();
                    if let Some(edit) = self.files.edit.as_mut() {
                        edit.text.push_str(&line);
                        edit.error = None;
                    }
                }
            }
            // AltGr llega como Ctrl+Alt: también escribe.
            _ if !m.control || m.alt => {
                if let Some(text) = key_char {
                    edit.text.extend(text.chars().filter(|c| !c.is_control()));
                    edit.error = None;
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Nuevo archivo o carpeta dentro de `target` (o junto a él si es un archivo).
    fn start_new(&mut self, target: &FileEntry, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fs) = self.files.fs.clone() else {
            return;
        };
        let parent = if target.is_dir {
            target.path.clone()
        } else {
            fs.parent(&target.path).unwrap_or_else(|| target.path.clone())
        };
        if self.files.dir.as_deref() != Some(parent.as_str()) && self.files.tree.expand(&parent) {
            self.load_files(false, cx);
        }
        self.files.edit = Some(InlineEdit {
            kind: EditKind::New { parent, is_dir },
            text: String::new(),
            error: None,
        });
        window.focus(&self.files.focus, cx);
        cx.notify();
    }

    fn start_file_rename(&mut self, entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.files.edit = Some(InlineEdit {
            text: entry.name.clone(),
            kind: EditKind::Rename { entry },
            error: None,
        });
        window.focus(&self.files.focus, cx);
        cx.notify();
    }

    /// Enter en el campo: crea o cambia el nombre fuera del hilo de la UI.
    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        let (Some(edit), Some(fs)) = (self.files.edit.clone(), self.files.fs.clone()) else {
            return;
        };
        let name = match validate_name(&edit.text) {
            Ok(n) => n,
            Err(reason) => {
                if let Some(e) = self.files.edit.as_mut() {
                    e.error = Some(reason);
                }
                cx.notify();
                return;
            }
        };
        let (parent, old) = match &edit.kind {
            EditKind::New { parent, .. } => (parent.clone(), None),
            EditKind::Rename { entry } => {
                if name == entry.name {
                    self.files.edit = None;
                    cx.notify();
                    return;
                }
                (fs.parent(&entry.path).unwrap_or_default(), Some(entry.path.clone()))
            }
        };
        let target = fs.join(&parent, &name);
        let kind = edit.kind.clone();
        let (target_bg, old_bg) = (target.clone(), old.clone());
        cx.spawn(async move |this, cx| {
            let result: Result<(), Option<String>> = cx
                .background_executor()
                .spawn(async move {
                    let exists = fs.stat(&target_bg).map_err(|e| Some(e.to_string()))?.is_some();
                    // Cambiar solo mayúsculas (en Windows "existe") es válido.
                    let case_only = old_bg
                        .as_ref()
                        .is_some_and(|o| o.to_lowercase() == target_bg.to_lowercase());
                    if exists && !case_only {
                        return Err(None);
                    }
                    match (&kind, &old_bg) {
                        (EditKind::New { is_dir: true, .. }, _) => fs.create_dir(&target_bg),
                        (EditKind::New { .. }, _) => fs.write(&target_bg, b""),
                        (EditKind::Rename { .. }, Some(o)) => fs.rename(o, &target_bg),
                        (EditKind::Rename { .. }, None) => Ok(()),
                    }
                    .map_err(|e| Some(e.to_string()))
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                match result {
                    Ok(()) => {
                        w.files.edit = None;
                        if let Some(o) = &old {
                            w.files.tree.forget(o);
                        }
                        w.files.selected = Some(target);
                        w.load_files(true, cx);
                    }
                    Err(None) => {
                        if let Some(e) = w.files.edit.as_mut() {
                            e.error = Some(T::NameExists);
                        }
                    }
                    Err(Some(e)) => {
                        w.files.edit = None;
                        w.files.notice = Some(format!("{}: {e}", t(T::NoticeFailed)));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Menú ────────────────────────────────────────────────────────────────

    /// Agente que corre en la sesión activa (prefiere el de la terminal principal): (¿acoplada?, id).
    fn active_agent(&self) -> Option<(bool, String)> {
        let sid = self.sessions.active_id()?;
        self.running
            .iter()
            .filter(|(s, _, _)| *s == sid)
            .min_by_key(|(_, docked, _)| *docked)
            .map(|(_, docked, a)| (*docked, a.agent.clone()))
    }

    /// Claude Code corriendo en la sesión activa: ¿en la acoplada?
    fn active_claude(&self) -> Option<bool> {
        let sid = self.sessions.active_id()?;
        self.running
            .iter()
            .filter(|(s, _, a)| *s == sid && a.agent == "claude")
            .map(|(_, docked, _)| *docked)
            .min()
    }

    fn menu_context(&self, entry: &FileEntry) -> MenuContext {
        MenuContext {
            is_dir: entry.is_dir,
            remote: self.files.remote,
            can_paste: self.files.clip.is_some(),
            agent: self.active_agent().is_some(),
            claude: self.active_claude().is_some(),
        }
    }

    /// Carpeta de destino de una acción sobre `entry` (ella misma o la que la contiene).
    fn dest_dir(&self, entry: &FileEntry) -> Option<String> {
        if entry.is_dir {
            return Some(entry.path.clone());
        }
        self.files.fs.as_ref()?.parent(&entry.path)
    }

    /// Ejecuta una opción del menú (o su atajo).
    fn menu_action(&mut self, action: MenuAction, entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = self.menu_context(&entry);
        self.files.menu = None;
        let fs = self.files.fs.clone();
        match action {
            MenuAction::Open => {
                if !ctx.remote {
                    cx.open_with_system(std::path::Path::new(&entry.path));
                }
            }
            MenuAction::OpenPreview => {
                if !ctx.remote {
                    self.open_preview(PathBuf::from(&entry.path), None, window, cx);
                }
            }
            MenuAction::NewFile => self.start_new(&entry, false, window, cx),
            MenuAction::NewFolder => self.start_new(&entry, true, window, cx),
            MenuAction::Reveal => {
                if !ctx.remote {
                    reveal_in_file_manager(&entry.path);
                }
            }
            MenuAction::ImagesPreview => self.open_images_preview(entry, window, cx),
            MenuAction::OpenInTerminal => {
                if let Some(dir) = self.dest_dir(&entry) {
                    self.open_terminal_at(dir, window, cx);
                }
            }
            MenuAction::Share => {}
            MenuAction::CopyGithubLink => self.copy_github_link(entry, cx),
            MenuAction::CopyRemoteAddress => {
                if let Some(label) = fs.as_ref().and_then(|f| f.remote_label()) {
                    let text = kuntur_files::share::remote_address(&label, &entry.path);
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.files_notice(t(T::NoticePathCopied), cx);
                }
            }
            // Próximamente (en gris en el menú).
            MenuAction::FindInFolder | MenuAction::OpenTimeline => {}
            MenuAction::AddToChat => {
                if let Some((docked, agent)) = self.active_agent() {
                    self.send_to_agent(&entry, &agent, docked, window, cx);
                } else {
                    self.files_notice(t(T::NoticeNoAgent), cx);
                }
            }
            MenuAction::ClaudeAddToChat => {
                if let Some(docked) = self.active_claude() {
                    self.send_to_agent(&entry, "claude", docked, window, cx);
                } else {
                    self.files_notice(t(T::NoticeNoAgent), cx);
                }
            }
            MenuAction::Cut | MenuAction::Copy => {
                if let Some(fs) = fs {
                    let cut = action == MenuAction::Cut;
                    self.files.clip = Some(FileClip {
                        fs,
                        remote: ctx.remote,
                        entries: vec![entry],
                        cut,
                    });
                    self.files_notice(t(if cut { T::NoticeCut } else { T::NoticeCopied }), cx);
                }
            }
            MenuAction::Paste => self.paste_into(&entry, cx),
            MenuAction::CopyPath => {
                cx.write_to_clipboard(ClipboardItem::new_string(entry.path.clone()));
                self.files_notice(t(T::NoticePathCopied), cx);
            }
            MenuAction::CopyRelativePath => {
                let rel = self
                    .files
                    .dir
                    .as_deref()
                    .and_then(|root| self.files.tree.style().relative(root, &entry.path))
                    .unwrap_or_else(|| entry.path.clone());
                cx.write_to_clipboard(ClipboardItem::new_string(rel));
                self.files_notice(t(T::NoticePathCopied), cx);
            }
            MenuAction::Rename => self.start_file_rename(entry, window, cx),
            MenuAction::Delete | MenuAction::WarningDelete => {
                self.files.confirm_delete = Some(ConfirmDelete {
                    entry,
                    remote: ctx.remote,
                });
            }
            MenuAction::Download => self.download(entry, cx),
            MenuAction::Upload => {
                if let Some(dir) = self.dest_dir(&entry) {
                    self.upload_into(dir, cx);
                }
            }
            MenuAction::AddPythonProject => {
                if let Some(dir) = self.dest_dir(&entry) {
                    let flavor = if ctx.remote || !cfg!(windows) {
                        ShellFlavor::Posix
                    } else {
                        ShellFlavor::from_program(None)
                    };
                    let delay = Duration::from_millis(if ctx.remote { 3_000 } else { 1_500 });
                    self.open_terminal_at(dir, window, cx);
                    if let Some(e) = self.sessions.active() {
                        e.value
                            .view
                            .read(cx)
                            .paste_text(python_venv_line(flavor).to_string(), delay);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Pestaña nueva en `dir` (remota: en el mismo servidor).
    fn open_terminal_at(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        let conn = self
            .sessions
            .active()
            .and_then(|e| e.value.remote.as_ref().map(|r| r.connection.clone()));
        match conn {
            Some(conn) if self.files.remote => {
                self.open_remote(&conn, Some(dir), None, None, window, cx);
                self.focus_active(window, cx);
            }
            _ => {
                self.exact.next_session_dir = Some(PathBuf::from(dir));
                self.new_session(window, cx);
                self.exact.next_session_dir = None;
            }
        }
    }

    /// Escribe (sin Enter) la ruta o `@ruta` en el agente de la sesión activa.
    fn send_to_agent(
        &mut self,
        entry: &FileEntry,
        agent: &str,
        docked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sid) = self.sessions.active_id() else {
            return;
        };
        let style = self.files.tree.style();
        let text = chat_reference(agent, &entry.path, self.files.follow.as_deref(), style);
        if let Some(view) = self.terminal(sid, docked).cloned() {
            view.read(cx).paste_text(text, Duration::ZERO);
            self.focus_active(window, cx);
            self.files_notice(t(T::NoticeSentToAgent), cx);
        }
    }

    /// "Compartir ▸ Copiar enlace de GitHub": consulta git fuera del hilo de la UI.
    fn copy_github_link(&mut self, entry: FileEntry, cx: &mut Context<Self>) {
        let Some(dir) = self.dest_dir(&entry) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let link = cx
                .background_executor()
                .spawn(async move {
                    let top = git_output(&dir, &["rev-parse", "--show-toplevel"])?;
                    let branch = git_output(&dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
                    let remote = git_output(&dir, &["remote", "get-url", "origin"])?;
                    let rel = PathStyle::Native.relative(&top, &entry.path)?;
                    kuntur_files::share::github_link(&remote, &branch, &rel, entry.is_dir)
                })
                .await;
            let _ = this.update(cx, |w, cx| match link {
                Some(link) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(link));
                    w.files_notice(t(T::NoticeLinkCopied), cx);
                }
                None => w.files_notice(t(T::NoticeNotGithub), cx),
            });
        })
        .detach();
    }

    /// Abre en la vista previa (F8) las imágenes de la carpeta (las primeras [`MAX_GALLERY`]).
    fn open_images_preview(&mut self, entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.remote || !entry.is_dir {
            return;
        }
        let dir = entry.path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let images: Vec<PathBuf> = cx
                .background_executor()
                .spawn(async move {
                    LocalFs
                        .list(&dir)
                        .map(|l| {
                            l.entries
                                .into_iter()
                                .filter(|e| {
                                    !e.is_dir
                                        && e.name
                                            .rsplit_once('.')
                                            .is_some_and(|(_, x)| IMAGE_EXTS.contains(&x.to_ascii_lowercase().as_str()))
                                })
                                .take(MAX_GALLERY)
                                .map(|e| PathBuf::from(e.path))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .await;
            let _ = this.update_in(cx, |w, window, cx| {
                if images.is_empty() {
                    w.files_notice(t(T::NoticeNoImages), cx);
                }
                for p in images {
                    w.open_preview(p, None, window, cx);
                }
            });
        })
        .detach();
    }

    /// Pega lo copiado/cortado en la carpeta de `target`.
    fn paste_into(&mut self, target: &FileEntry, cx: &mut Context<Self>) {
        let (Some(clip), Some(dst), Some(dest_dir)) =
            (self.files.clip.clone(), self.files.fs.clone(), self.dest_dir(target))
        else {
            return;
        };
        let same_fs = clip.remote == self.files.remote && clip.fs.remote_label() == dst.remote_label();
        if clip.cut && same_fs {
            // Mover dentro del mismo sistema: renombrar (con nombre libre si ya existe).
            let style = self.files.tree.style();
            if clip
                .entries
                .iter()
                .any(|e| e.is_dir && style.is_within(&e.path, &dest_dir))
            {
                self.files_notice(t(T::NoticeMoveInside), cx);
                return;
            }
            let entries = clip.entries.clone();
            self.files.clip = None;
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        for e in &entries {
                            let mut to = dst.join(&dest_dir, &e.name);
                            if to == e.path {
                                continue;
                            }
                            if dst.exists(&to)? {
                                let name =
                                    conflict_name(&e.name, |n| dst.exists(&dst.join(&dest_dir, n)).unwrap_or(true));
                                to = dst.join(&dest_dir, &name);
                            }
                            dst.rename(&e.path, &to)?;
                        }
                        Ok::<_, std::io::Error>(entries)
                    })
                    .await;
                let _ = this.update(cx, |w, cx| {
                    match result {
                        Ok(entries) => {
                            for e in &entries {
                                w.files.tree.forget(&e.path);
                            }
                        }
                        Err(e) => w.files.notice = Some(format!("{}: {e}", t(T::NoticeFailed))),
                    }
                    w.load_files(true, cx);
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        // Copiar (o mover entre sistemas distintos) por la cola de transferencias.
        let direction = match (clip.remote, self.files.remote) {
            (true, false) => Direction::Download,
            (false, true) => Direction::Upload,
            _ => Direction::Copy,
        };
        for e in &clip.entries {
            let job = self
                .files
                .transfers
                .enqueue(direction, clip.fs.clone(), &e.path, dst.clone(), &dest_dir);
            if clip.cut {
                self.files.cut_jobs.push(CutJob {
                    job,
                    fs: clip.fs.clone(),
                    path: e.path.clone(),
                });
            }
        }
        if clip.cut {
            self.files.clip = None;
        }
        self.start_transfer_poll(cx);
        cx.notify();
    }

    /// Cortar entre sistemas: cuando la copia terminó bien, se borra el original.
    fn finish_cut_jobs(&mut self, cx: &mut Context<Self>) {
        let jobs = self.files.transfers.snapshot();
        let mut i = 0;
        while i < self.files.cut_jobs.len() {
            let state = jobs
                .iter()
                .find(|j| j.id == self.files.cut_jobs[i].job)
                .map(|j| j.state.clone());
            match state {
                Some(s) if !s.is_finished() => i += 1,
                Some(kuntur_files::JobState::Done) => {
                    let c = self.files.cut_jobs.remove(i);
                    cx.background_executor()
                        .spawn(async move {
                            let _ = c.fs.remove_all(&c.path);
                        })
                        .detach();
                }
                // Falló, se canceló o se omitió: el original se queda.
                _ => {
                    self.files.cut_jobs.remove(i);
                }
            }
        }
    }

    // ── Transferencias ──────────────────────────────────────────────────────

    /// "Descargar esto": elige una carpeta local y encola la descarga.
    fn download(&mut self, entry: FileEntry, cx: &mut Context<Self>) {
        self.files.menu = None;
        let Some(fs) = self.files.fs.clone().filter(|_| self.files.remote) else {
            return;
        };
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t(T::PickDownloadHere).into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dest) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |w, cx| {
                let dest = dest.to_string_lossy().into_owned();
                w.files
                    .transfers
                    .enqueue(Direction::Download, fs, &entry.path, Arc::new(LocalFs), &dest);
                w.start_transfer_poll(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// "Subir esto": elige archivos locales y los sube a `dir` del servidor.
    fn upload_into(&mut self, dir: String, cx: &mut Context<Self>) {
        let Some(fs) = self.files.fs.clone().filter(|_| self.files.remote) else {
            return;
        };
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(t(T::PickUploadHere).into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let _ = this.update(cx, |w, cx| {
                let local: Arc<dyn FileSystem> = Arc::new(LocalFs);
                for p in paths {
                    w.files
                        .transfers
                        .enqueue(Direction::Upload, local.clone(), &p.to_string_lossy(), fs.clone(), &dir);
                }
                w.start_transfer_poll(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Archivos soltados desde el Explorador sobre `dest` (o la raíz): subir (remoto) o copiar
    /// (local).
    fn drop_paths(&mut self, paths: &[PathBuf], dest: Option<String>, cx: &mut Context<Self>) {
        self.files.menu = None;
        let (Some(fs), Some(dest)) = (self.files.fs.clone(), dest.or_else(|| self.files.dir.clone())) else {
            self.files.notice = Some(if self.files.remote {
                "Sin conexión: no se puede subir".into()
            } else {
                "Carpeta desconocida".into()
            });
            cx.notify();
            return;
        };
        let direction = if self.files.remote {
            Direction::Upload
        } else {
            Direction::Copy
        };
        let local: Arc<dyn FileSystem> = Arc::new(LocalFs);
        for p in paths {
            self.files
                .transfers
                .enqueue(direction, local.clone(), &p.to_string_lossy(), fs.clone(), &dest);
        }
        self.start_transfer_poll(cx);
        cx.notify();
    }

    /// Sube a la carpeta temporal "para el agente" de la pestaña y, al terminar, escribe las rutas
    /// en su terminal (§7).
    fn upload_for_agent(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let (Some(fs), Some(entry)) = (
            self.files.fs.clone().filter(|_| self.files.remote),
            self.sessions.active(),
        ) else {
            return;
        };
        let (session, dest_dir) = (entry.id, agent_dir(&entry.name));
        let local: Arc<dyn FileSystem> = Arc::new(LocalFs);
        let jobs = paths
            .iter()
            .map(|p| {
                self.files.transfers.enqueue(
                    Direction::Upload,
                    local.clone(),
                    &p.to_string_lossy(),
                    fs.clone(),
                    &dest_dir,
                )
            })
            .collect();
        self.files.agent_groups.push(AgentGroup {
            session,
            dest_dir,
            jobs,
        });
        self.start_transfer_poll(cx);
        cx.notify();
    }

    /// Botón del agente: elegir archivos con el diálogo del sistema.
    fn pick_for_agent(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Subir para el agente".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |w, cx| w.upload_for_agent(&paths, cx));
            }
        })
        .detach();
    }

    /// Grupos "para el agente" terminados: rutas a la terminal de su pestaña y al portapapeles.
    fn finish_agent_groups(&mut self, cx: &mut Context<Self>) {
        let jobs = self.files.transfers.snapshot();
        let mut i = 0;
        while i < self.files.agent_groups.len() {
            let g = &self.files.agent_groups[i];
            let mine: Vec<_> = jobs.iter().filter(|j| g.jobs.contains(&j.id)).collect();
            if mine.iter().any(|j| !j.state.is_finished()) {
                i += 1;
                continue;
            }
            let g = self.files.agent_groups.remove(i);
            let paths: Vec<String> = mine
                .iter()
                .filter(|j| j.state == kuntur_files::JobState::Done)
                .map(|j| kuntur_files::PathStyle::Posix.join(&g.dest_dir, &j.name))
                .collect();
            if paths.is_empty() {
                continue;
            }
            let text = format!("{} ", posix_quote_all(&paths));
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            if let Some(e) = self.sessions.get(g.session) {
                e.value.view.read(cx).paste_text(text, Duration::ZERO);
            }
            self.files.notice = Some(format!("Rutas para el agente en {} (también copiadas)", g.dest_dir));
        }
    }

    /// Repinta la cola mientras haya algo en curso; al terminar un trabajo relee el árbol.
    pub(super) fn start_transfer_poll(&mut self, cx: &mut Context<Self>) {
        if self.files.poll.is_some() {
            return;
        }
        self.files.poll = Some(cx.spawn(async move |this, cx| {
            let mut finished = usize::MAX;
            loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let keep = this.update(cx, |w, cx| {
                    let s = w.files.transfers.summary();
                    if s.finished != finished {
                        finished = s.finished;
                        w.finish_agent_groups(cx);
                        w.finish_cut_jobs(cx);
                        w.load_files(true, cx);
                    }
                    cx.notify();
                    s.active > 0
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |w, _| w.files.poll = None);
        }));
    }

    // ── Eliminar ────────────────────────────────────────────────────────────

    /// Local: a la Papelera; remoto: se borra en el servidor (sin papelera).
    fn delete_confirmed(&mut self, cx: &mut Context<Self>) {
        let (Some(c), Some(fs)) = (self.files.confirm_delete.take(), self.files.fs.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let path = c.entry.path.clone();
            let remote = c.remote;
            let result = cx
                .background_executor()
                .spawn(async move {
                    if remote {
                        fs.remove_all(&path).map_err(|e| e.to_string())
                    } else {
                        trash::delete(&path).map_err(|e| e.to_string())
                    }
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.files.notice = Some(match result {
                    Ok(()) => format!("{} «{}»", t(T::NoticeDeleted), c.entry.name),
                    Err(e) => format!("{} «{}»: {e}", t(T::NoticeFailed), c.entry.name),
                });
                w.files.tree.forget(&c.entry.path);
                if w.files.selected.as_deref() == Some(c.entry.path.as_str()) {
                    w.files.selected = None;
                }
                w.load_files(true, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // ── Prueba de humo ──────────────────────────────────────────────────────

    /// `KUNTUR_SMOKE_FILES=remote`: panel "Archivos" con un árbol remoto **sintético** (en memoria)
    /// y una cola de transferencias sintética. Requiere una pestaña remota (`-Connections remote`).
    pub(super) fn smoke_files(&mut self, cx: &mut Context<Self>) {
        let Ok(mode) = std::env::var("KUNTUR_SMOKE_FILES") else {
            return;
        };
        self.config_path = None;
        self.settings.sidebar = SidebarMode::Full;
        self.panel = PanelView::Files;
        if mode == "remote" {
            let now = now_secs() as i64;
            let fs = MemoryFs::new(Some("demo@127.0.0.1:1"), "/home/demo");
            for (path, size, age) in [
                ("/home/demo/proyecto/src/main.rs", 4_200, 600),
                ("/home/demo/proyecto/README.md", 2_150, 3_600),
                ("/home/demo/proyecto/Cargo.toml", 512, 86_400),
                ("/home/demo/proyecto/datos/ventas.csv", 1_800_000, 7 * 86_400),
                ("/home/demo/proyecto/informe-final.pdf", 3_400_000, 90),
                ("/home/demo/proyecto/.env.ejemplo", 96, 30 * 86_400),
            ] {
                fs.add_file(path, &vec![b' '; size], now - age);
            }
            let _ = fs.create_dir("/home/demo/proyecto/docs");
            self.files.smoke_fs = Some(Arc::new(fs));
            self.files.transfers.with_queue(|q| {
                use kuntur_files::JobState;
                q.push_synthetic(
                    Direction::Upload,
                    "capturas-del-error.zip",
                    "/home/demo/proyecto",
                    12_000_000,
                    5_100_000,
                    JobState::Running,
                );
                q.push_synthetic(
                    Direction::Upload,
                    "ventas.csv",
                    "/home/demo/proyecto/datos",
                    1_800_000,
                    0,
                    JobState::Asking,
                );
                q.push_synthetic(
                    Direction::Download,
                    "README.md",
                    "Descargas",
                    2_150,
                    2_150,
                    JobState::Done,
                );
            });
        }
        self.sync_files(true, cx);
        if self.files.smoke_fs.is_some() {
            // Dentro de la carpeta del proyecto sintético, con `src` desplegada.
            self.files.dir = Some("/home/demo/proyecto".into());
            self.files.tree.set_root(self.files.dir.clone());
            self.files.tree.expand("/home/demo/proyecto/src");
            self.load_files(false, cx);
        }
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    pub(super) fn render_files_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let remote_label = self
            .sessions
            .active()
            .filter(|e| e.value.remote.is_some())
            .and_then(|e| self.remote_label(e.id));
        let tag = location_label(remote_label.as_deref());

        let blocked = self.files.remote && self.files.fs_error.is_some();
        let Some(dir) = self.files.dir.clone().filter(|_| !blocked) else {
            let (title, hint): (&'static str, String) = match (&remote_label, &self.files.fs_error) {
                (Some(_), Some(e)) => (
                    "Sin conexión con el servidor",
                    format!("{e} · el árbol aparece al conectar"),
                ),
                (Some(_), None) => ("Conectando…", String::new()),
                (None, _) => ("Carpeta desconocida", "Abre un shell PowerShell en la sesión".into()),
            };
            return v_flex()
                .flex_1()
                .min_h_0()
                .child(self.render_files_header(None, &tag, cx))
                .child(empty_note(c, title, hint))
                .child(self.render_transfers(cx))
                .into_any_element();
        };

        let accent = c.accent;
        let drop_dir = dir.clone();
        let rows = self.view_rows();
        let body: AnyElement = match self.files.tree.root_state() {
            None => empty_note(c, t(T::FilesLoading), "").into_any_element(),
            Some(DirState::Error(_)) => {
                empty_note(c, t(T::FilesReadError), t(T::FilesReadErrorHint)).into_any_element()
            }
            Some(DirState::Loaded(l)) if l.entries.is_empty() && rows.is_empty() => empty_note(
                c,
                t(T::FilesEmpty),
                if self.files.remote {
                    t(T::FilesDropUpload)
                } else {
                    t(T::FilesDropCopy)
                },
            )
            .into_any_element(),
            Some(DirState::Loaded(l)) => {
                let count = rows.len();
                let (truncated, shown) = (l.truncated, l.entries.len());
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "files",
                            count,
                            cx.processor(|this: &mut Self, range: Range<usize>, _window, cx| this.file_rows(range, cx)),
                        )
                        .track_scroll(&self.files.scroll)
                        .flex_1()
                        .px(px(4.))
                        .py(px(4.)),
                    )
                    .when(truncated, |this| {
                        this.child(
                            div()
                                .px(px(14.))
                                .py(px(6.))
                                .text_size(px(11.))
                                .text_color(c.muted)
                                .child(format!("Se muestran los primeros {shown} elementos")),
                        )
                    })
                    .into_any_element()
            }
        };
        // Soltar sobre la lista (fuera de una carpeta) = a la raíz. Clic en el vacío = foco del árbol.
        let body = div()
            .id("files-drop")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(accent.opacity(0.08)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                this.drop_paths(paths.paths(), Some(drop_dir.clone()), cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    // Clic derecho en el vacío: el menú de la raíz.
                    if let Some(root) = this.root_entry() {
                        this.open_file_menu(root, ev.position, window, cx);
                    }
                }),
            )
            .child(body);

        v_flex()
            .id("files-view")
            .track_focus(&self.files.focus)
            .flex_1()
            .min_h_0()
            .child(self.render_files_header(Some(&dir), &tag, cx))
            .child(body)
            .children(self.render_delete_confirm(cx))
            .children(self.render_agent_zone(cx))
            .child(self.render_transfers(cx))
            .children(self.render_files_notice())
            .into_any_element()
    }

    /// Encabezado: ↑, nombre de la raíz, plegar todo y, debajo, la etiqueta local/remoto y la ruta.
    fn render_files_header(&self, dir: Option<&str>, tag: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let fs = self.files.fs.as_ref();
        let name = dir
            .map(|d| fs.and_then(|f| f.file_name(d)).unwrap_or_else(|| d.to_string()))
            .unwrap_or_else(|| t(T::FilesTitle).into());
        let full = dir
            .map(|d| {
                if self.files.remote {
                    d.to_string()
                } else {
                    kuntur_files::display_path(std::path::Path::new(d), self.home.as_deref())
                }
            })
            .unwrap_or_default();
        let has_parent = self.files_parent().is_some();
        let away = dir.is_some() && self.files.follow.as_deref().is_some_and(|f| Some(f) != dir);
        let remote = self.files.remote;
        let tip = full.clone();

        let top = h_flex()
            .h(px(30.))
            .gap(px(2.))
            .items_center()
            .child(
                icon_button("files-up", IconName::ArrowUp, c, t(T::FilesUp))
                    .when(!has_parent, |this| this.opacity(0.35).cursor_default())
                    .on_click(cx.listener(|this, _, _, cx| this.files_up(cx))),
            )
            .child(
                div()
                    .id("files-dir")
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(name),
            )
            .when(away, |this| {
                this.child(
                    icon_button("files-home", IconName::SquareTerminal, c, t(T::FilesBackToSession)).on_click(
                        cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.files.follow.clone() {
                                this.open_dir(f, cx);
                            }
                        }),
                    ),
                )
            })
            .child(
                icon_button("files-collapse", IconName::ChevronsUpDown, c, t(T::FilesCollapseAll))
                    .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
            )
            .when(remote && self.files.fs.is_some(), |this| {
                this.child(
                    icon_button("files-agent", IconName::Bot, c, "Subir archivos para el agente…")
                        .on_click(cx.listener(|this, _, _, cx| this.pick_for_agent(cx))),
                )
            });

        let chip_color = if remote { c.accent } else { c.muted };
        let bottom = h_flex()
            .h(px(18.))
            .pl(px(6.))
            .gap(px(6.))
            .items_center()
            .text_size(px(11.))
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(6.))
                    .rounded(px(4.))
                    .bg(chip_color.opacity(0.14))
                    .text_color(chip_color)
                    .child(tag.to_string()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(c.muted)
                    .child(full),
            );

        v_flex()
            .flex_shrink_0()
            .pl(px(6.))
            .pr(px(6.))
            .pt(px(5.))
            .pb(px(6.))
            .border_b_1()
            .border_color(c.border)
            .child(top)
            .child(bottom)
            .into_any_element()
    }

    /// Guías verticales de la sangría (una por nivel).
    fn indent_guides(&self, depth: usize) -> impl Iterator<Item = Div> + '_ {
        let color = self.colors.border;
        (0..depth).map(move |level| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(ROW_PAD + level as f32 * INDENT + 6.))
                .w(px(1.))
                .bg(color)
        })
    }

    fn file_rows(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rows = self.view_rows();
        let end = range.end.min(rows.len());
        let start = range.start.min(end);
        rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| match row {
                ViewRow::Entry(r) => self.render_tree_row(start + i, r, cx),
                ViewRow::NewInput { depth, is_dir } => self.render_new_input(start + i, *depth, *is_dir),
            })
            .collect()
    }

    fn render_tree_row(&self, index: usize, row: &TreeRow, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let entry = &row.entry;
        let selected = self.files.selected.as_ref() == Some(&entry.path);
        let renaming = match &self.files.edit {
            Some(InlineEdit {
                kind: EditKind::Rename { entry: e },
                ..
            }) if e.path == entry.path => self.files.edit.clone(),
            _ => None,
        };
        let (hover, accent) = (c.hover, c.accent);
        let (click, right, target) = (entry.clone(), entry.clone(), entry.path.clone());
        let tip: SharedString = {
            let mut parts = vec![entry.path.clone()];
            if !entry.is_dir {
                parts.push(format_size(entry.size));
            }
            if let Some(m) = entry.modified {
                parts.push(format_modified(m, now_secs() as i64, &jiff::tz::TimeZone::system()));
            }
            if let Some(e) = &row.error {
                parts.push(e.clone());
            }
            parts.join(" · ").into()
        };
        let chevron: AnyElement = if entry.is_dir {
            Icon::new(if row.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size(px(12.))
            .text_color(if row.loading { c.muted.opacity(0.5) } else { c.muted })
            .into_any_element()
        } else {
            div().into_any_element()
        };
        let name: AnyElement = match renaming {
            Some(edit) => self.render_edit_box(&edit).into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(if row.error.is_some() { c.error } else { c.text })
                .child(entry.name.clone())
                .into_any_element(),
        };
        h_flex()
            .id(("file", index))
            .relative()
            .w_full()
            .h(px(ROW))
            .pl(px(ROW_PAD + row.depth as f32 * INDENT))
            .pr(px(6.))
            .gap(px(4.))
            .items_center()
            .rounded(px(5.))
            .cursor_pointer()
            .map(|this| {
                if selected {
                    this.bg(accent.opacity(0.16))
                } else {
                    this.hover(move |s| s.bg(hover))
                }
            })
            .children(self.indent_guides(row.depth))
            // Soltar sobre una carpeta = a esa carpeta.
            .when(entry.is_dir, |this| {
                this.drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(accent.opacity(0.24)))
                    .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                        this.drop_paths(paths.paths(), Some(target.clone()), cx)
                    }))
            })
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| this.on_row_click(&click, ev, window, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_file_menu(right.clone(), ev.position, window, cx)
                }),
            )
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(div().w(px(12.)).flex_shrink_0().flex().justify_center().child(chevron))
            .child(
                Icon::new(file_icon(entry, row.expanded))
                    .size(px(14.))
                    .flex_shrink_0()
                    .text_color(if entry.is_dir { c.accent } else { c.muted }),
            )
            .child(name)
            .into_any_element()
    }

    /// Fila del nombre nuevo (archivo o carpeta) dentro de su carpeta.
    fn render_new_input(&self, index: usize, depth: usize, is_dir: bool) -> AnyElement {
        let c = &self.colors;
        let edit = self.files.edit.clone();
        h_flex()
            .id(("file-new", index))
            .relative()
            .w_full()
            .h(px(ROW))
            .pl(px(ROW_PAD + depth as f32 * INDENT))
            .pr(px(6.))
            .gap(px(4.))
            .items_center()
            .children(self.indent_guides(depth))
            .child(div().w(px(12.)).flex_shrink_0())
            .child(
                Icon::new(if is_dir { IconName::Folder } else { IconName::File })
                    .size(px(14.))
                    .flex_shrink_0()
                    .text_color(c.muted),
            )
            .children(edit.map(|e| self.render_edit_box(&e)))
            .into_any_element()
    }

    /// Caja del campo en línea: el texto, el cursor y, si hay, el motivo del error.
    fn render_edit_box(&self, edit: &InlineEdit) -> Stateful<Div> {
        let c = &self.colors;
        let hint = match (&edit.error, &edit.kind) {
            (Some(e), _) => t(*e),
            (None, EditKind::New { .. }) => t(T::FilesNewFileHint),
            (None, EditKind::Rename { .. }) => t(T::FilesRenameHint),
        };
        let border = if edit.error.is_some() { c.error } else { c.accent };
        div()
            .id("files-edit")
            .flex_1()
            .min_w_0()
            .h(px(20.))
            .px(px(4.))
            .flex()
            .items_center()
            .rounded(px(3.))
            .border_1()
            .border_color(border)
            .bg(c.surface)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_color(c.text)
            .tooltip(move |window, cx| Tooltip::new(hint).build(window, cx))
            .child(edit.text.clone())
            .child(div().w(px(1.)).h(px(14.)).bg(c.text))
    }

    /// Menú del clic derecho (y su submenú "Compartir ▸"), sobre un fondo que lo cierra al hacer clic
    /// fuera. Se dibuja en la raíz de la ventana.
    pub(super) fn render_file_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.files.menu.as_ref()?;
        let c = &self.colors;
        let entries = build_menu(self.menu_context(&menu.entry));
        let mut share_offset = MENU_PAD;
        let mut found = false;
        for e in &entries {
            match e {
                MenuEntry::Item(i) if i.action == MenuAction::Share => {
                    found = true;
                    break;
                }
                MenuEntry::Item(_) => share_offset += MENU_ITEM_H,
                MenuEntry::Separator => share_offset += MENU_SEP_H,
            }
        }
        let rows = entries.iter().enumerate().map(|(i, e)| match e {
            MenuEntry::Separator => div().h(px(1.)).my(px(4.)).mx(px(6.)).bg(c.border).into_any_element(),
            MenuEntry::Item(item) => self.menu_row(("menu-item", i), *item, menu.entry.clone(), cx),
        });
        let list = menu_panel(c).children(rows);
        let share = (menu.share_open && found).then(|| {
            let remote = self.files.remote;
            let sub = share_menu(remote)
                .into_iter()
                .enumerate()
                .map(|(i, item)| self.menu_row(("share-item", i), item, menu.entry.clone(), cx));
            anchored()
                .position(point(
                    menu.position.x + px(MENU_W - 6.),
                    menu.position.y + px(share_offset - MENU_PAD),
                ))
                .snap_to_window_with_margin(px(8.))
                .child(menu_panel(c).w(px(230.)).children(sub))
        });
        Some(
            deferred(
                div()
                    .id("files-menu-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close_file_menu(cx)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.close_file_menu(cx)),
                    )
                    .child(
                        anchored()
                            .position(menu.position)
                            .snap_to_window_with_margin(px(8.))
                            .child(list),
                    )
                    .children(share),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Una opción del menú: texto, "(próximamente)", atajo a la derecha y ▸ si abre un submenú.
    fn menu_row(
        &self,
        id: (&'static str, usize),
        item: MenuItem,
        entry: FileEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let label: SharedString = if item.soon {
            format!("{} {}", t(item.label), t(T::Soon)).into()
        } else {
            t(item.label).into()
        };
        h_flex()
            .id(id)
            .h(px(MENU_ITEM_H))
            .px(px(12.))
            .gap(px(16.))
            .items_center()
            .rounded(px(5.))
            .map(|this| {
                if item.enabled {
                    this.cursor_pointer().text_color(c.text).hover(move |s| s.bg(hover))
                } else {
                    this.text_color(c.muted).opacity(0.55)
                }
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if let Some(m) = this.files.menu.as_mut() {
                    let open = item.submenu;
                    // El submenú se abre al pasar sobre "Compartir" y se cierra al pasar sobre otra
                    // opción del menú principal.
                    if *hovered && id.0 == "menu-item" && m.share_open != open {
                        m.share_open = open;
                        cx.notify();
                    }
                }
            }))
            .when(item.enabled, |this| {
                this.on_click(cx.listener(move |this, _, window, cx| {
                    if item.submenu {
                        if let Some(m) = this.files.menu.as_mut() {
                            m.share_open = true;
                        }
                        cx.notify();
                        return;
                    }
                    this.menu_action(item.action, entry.clone(), window, cx);
                }))
            })
            .child(div().flex_1().min_w_0().whitespace_nowrap().child(label))
            .when_some(item.shortcut, |this, k| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.5))
                        .text_color(c.muted)
                        .child(t(k)),
                )
            })
            .when(item.submenu, |this| {
                this.child(Icon::new(IconName::ChevronRight).size(px(12.)).text_color(c.muted))
            })
            .into_any_element()
    }

    /// Confirmación al pie del árbol: a la Papelera (local) o del servidor sin papelera (remoto).
    fn render_delete_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirm = self.files.confirm_delete.as_ref()?;
        let c = &self.colors;
        let (title, hint) = if confirm.remote {
            (t(T::ConfirmRemoteDelete), t(T::ConfirmRemoteDeleteHint))
        } else {
            (t(T::ConfirmTrash), t(T::ConfirmTrashHint))
        };
        Some(
            v_flex()
                .flex_shrink_0()
                .mx(px(8.))
                .mb(px(6.))
                .p(px(8.))
                .gap(px(6.))
                .rounded(px(8.))
                .border_1()
                .border_color(c.error.opacity(0.5))
                .text_size(px(12.))
                .child(
                    div()
                        .text_color(c.text)
                        .child(format!("{title} «{}»", confirm.entry.name)),
                )
                .child(div().text_size(px(11.)).text_color(c.muted).child(hint))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .justify_end()
                        .child(
                            small_button("delete-cancel", t(T::ButtonCancel), c.text, c).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.files.confirm_delete = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            small_button("delete-ok", t(T::ButtonDelete), c.error, c)
                                .on_click(cx.listener(|this, _, _, cx| this.delete_confirmed(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Zona "para el agente" (solo remoto, con SFTP): soltar = subir a la carpeta temporal.
    fn render_agent_zone(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.files.remote || self.files.fs.is_none() {
            return None;
        }
        let c = &self.colors;
        let dir = agent_dir(&self.sessions.active()?.name);
        let accent = c.accent;
        Some(
            h_flex()
                .id("agent-drop")
                .flex_shrink_0()
                .mx(px(8.))
                .mb(px(6.))
                .px(px(8.))
                .py(px(6.))
                .gap(px(8.))
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(c.border)
                .text_size(px(11.))
                .text_color(c.muted)
                .cursor_pointer()
                .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(accent.opacity(0.18)).border_color(accent))
                .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.upload_for_agent(paths.paths(), cx)))
                .on_click(cx.listener(|this, _, _, cx| this.pick_for_agent(cx)))
                .tooltip(|window, cx| {
                    Tooltip::new("Sube a una carpeta temporal del servidor y escribe las rutas en la terminal")
                        .build(window, cx)
                })
                .child(Icon::new(IconName::Bot).size(px(14.)).flex_shrink_0())
                .child(
                    v_flex()
                        .min_w_0()
                        .child(div().text_color(c.text).child("Soltar aquí para el agente"))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(format!("{dir}/")),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_files_notice(&self) -> Option<AnyElement> {
        let text = self.files.notice.clone()?;
        let c = &self.colors;
        Some(
            div()
                .flex_shrink_0()
                .px(px(14.))
                .pb(px(6.))
                .text_size(px(11.))
                .text_color(c.muted)
                .child(text)
                .into_any_element(),
        )
    }
}

/// Panel del menú (y del submenú): fondo, borde y sombra; los clics dentro no lo cierran.
fn menu_panel(c: &UiColors) -> Div {
    v_flex()
        .w(px(MENU_W))
        .p(px(MENU_PAD))
        .rounded(px(8.))
        .border_1()
        .border_color(c.border)
        .bg(c.popover)
        .shadow_lg()
        .text_size(px(12.5))
        .text_color(c.text)
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
}

/// Botón de texto chico (confirmaciones y conflictos).
pub(super) fn small_button(id: impl Into<ElementId>, label: &'static str, color: Hsla, c: &UiColors) -> Stateful<Div> {
    let hover = c.hover;
    div()
        .id(id)
        .h(px(22.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(c.border)
        .text_size(px(11.5))
        .text_color(color)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label)
}

/// Aviso centrado en el panel: título y, opcional, una línea tenue.
pub(super) fn empty_note(c: &UiColors, title: impl Into<SharedString>, hint: impl Into<SharedString>) -> Div {
    let hint: SharedString = hint.into();
    v_flex()
        .flex_1()
        .items_center()
        .pt(px(48.))
        .px(px(20.))
        .gap(px(4.))
        .text_center()
        .child(
            div()
                .text_size(px(12.5))
                .text_color(c.text)
                .opacity(0.85)
                .child(title.into()),
        )
        .when(!hint.is_empty(), |this| {
            this.child(div().text_size(px(11.5)).text_color(c.muted).child(hint))
        })
}

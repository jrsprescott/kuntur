//! Contenido "Archivos" del panel izquierdo: la carpeta actual de la sesión **activa**, local o
//! remota por SFTP (F7), **nunca mezcladas**: el encabezado muestra siempre la ruta y la etiqueta
//! "local" o "remoto · usuario@host".
//!
//! - Clic en carpeta entra, ↑ sube, doble clic en un archivo local lo abre con su programa.
//! - Clic derecho: "Descargar…" (remoto), "Abrir" (local), "Copiar ruta", "Borrar…" (con
//!   confirmación).
//! - Soltar archivos del Explorador sobre una carpeta (o sobre la lista) = subir por SFTP a esa
//!   carpeta (remoto) o copiarlos (local). La cola está en `transfers_view.rs`.
//! - Remoto: zona "para el agente" que sube a `/tmp/kuntur-<sesión>/` y escribe las rutas en la
//!   terminal (§7).
//!
//! Toda lectura y escritura corre fuera del hilo de la UI.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use kuntur_files::{
    Direction, FileEntry, FileSystem, JobId, Listing, LocalFs, MemoryFs, Transfers, agent_dir, format_modified,
    format_size, location_label, posix_quote_all,
};
use kuntur_ssh::RemoteSession;

use super::*;

const ROW: f32 = 26.0;
/// Columna de la fecha (alineada a la derecha).
const DATE_COL: f32 = 50.0;
const MENU_W: f32 = 190.0;

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
}

/// Subidas "para el agente" de una pestaña: al terminar, sus rutas se escriben en la terminal.
struct AgentGroup {
    session: u64,
    dest_dir: String,
    jobs: Vec<JobId>,
}

#[derive(Default)]
pub(super) struct FilesState {
    /// Pestaña cuyo árbol se muestra: al cambiar de pestaña, todo el estado se reinicia.
    session: Option<u64>,
    /// ¿La pestaña es remota? (se fija junto con `session`).
    remote: bool,
    /// Sistema de archivos de la pestaña (local, SFTP o sintético en las pruebas de humo).
    fs: Option<Arc<dyn FileSystem>>,
    /// Remota sin SFTP: el motivo ("sin conexión con el servidor", "conectando…").
    fs_error: Option<String>,
    /// Carpeta que se muestra (puede ser otra si el usuario navegó).
    dir: Option<String>,
    /// Carpeta de la sesión la última vez que se sincronizó: si la sesión cambia de carpeta, el
    /// panel la sigue.
    follow: Option<String>,
    listing: Option<Result<Listing, String>>,
    selected: Option<String>,
    /// Lectura en curso (se cancela al soltarla).
    task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
    menu: Option<FileMenu>,
    /// Entrada que pide confirmar el borrado.
    confirm_delete: Option<FileEntry>,
    /// Aviso corto al pie (ruta copiada, error al borrar…).
    pub(super) notice: Option<String>,
    /// Cola de transferencias (todas las pestañas).
    pub(super) transfers: Transfers,
    /// Repinta mientras haya transferencias en curso.
    poll: Option<Task<()>>,
    agent_groups: Vec<AgentGroup>,
    /// Prueba de humo: árbol remoto **sintético** (nunca un servidor real).
    smoke_fs: Option<Arc<dyn FileSystem>>,
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

    /// Sigue la carpeta de la sesión activa y vuelve a leer la carpeta mostrada. Solo trabaja con el
    /// panel en "Archivos" y visible; `force` relee aunque nada haya cambiado.
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
            self.files.listing = None;
            self.files.selected = None;
            self.files.menu = None;
            self.files.confirm_delete = None;
            self.files.task = None;
        }
        if cwd.is_some() && cwd != self.files.follow {
            self.files.follow = cwd.clone();
            self.files.dir = cwd;
            self.files.selected = None;
            self.files.listing = None;
        }
        // Mientras hay una lectura en curso no se lanza otra (salvo que la carpeta cambie arriba).
        if force || self.files.task.is_none() || self.files.listing.is_none() {
            self.load_files(cx);
        }
    }

    fn load_files(&mut self, cx: &mut Context<Self>) {
        let Some((id, source)) = self.files_source(cx) else {
            return;
        };
        if self.files.session != Some(id) {
            return;
        }
        let dir = self.files.dir.clone();
        // Local: sin carpeta conocida no hay nada que leer.
        let remote = match source {
            FilesSource::Local { .. } if dir.is_none() => {
                self.files.task = None;
                return;
            }
            FilesSource::Local { .. } => None,
            FilesSource::Remote { session, .. } => Some(session),
        };
        let smoke = self.files.smoke_fs.clone();
        let requested = dir.clone();
        self.files.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let fs: Arc<dyn FileSystem> = match (remote, smoke) {
                        (None, _) => Arc::new(LocalFs),
                        (Some(_), Some(fake)) => fake,
                        (Some(session), None) => session.sftp().map_err(|e| e.to_string())?,
                    };
                    let dir = match dir {
                        Some(d) => d,
                        None => fs.home().map_err(|e| e.to_string())?,
                    };
                    let listing = fs.list(&dir).map_err(|e| e.to_string());
                    Ok::<_, String>((fs, dir, listing))
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.files.task = None;
                if w.files.session != Some(id) || w.files.dir != requested {
                    return;
                }
                match result {
                    Ok((fs, dir, listing)) => {
                        w.files.fs = Some(fs);
                        w.files.fs_error = None;
                        w.files.dir = Some(dir);
                        if w.files.listing.as_ref() != Some(&listing) {
                            w.files.listing = Some(listing);
                        }
                    }
                    Err(e) => {
                        w.files.fs = None;
                        w.files.fs_error = Some(e);
                        w.files.listing = None;
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_dir(&mut self, dir: String, cx: &mut Context<Self>) {
        self.files.dir = Some(dir);
        self.files.selected = None;
        self.files.listing = None;
        self.files.menu = None;
        self.files.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.load_files(cx);
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

    fn on_file_click(&mut self, entry: &FileEntry, ev: &ClickEvent, cx: &mut Context<Self>) {
        self.files.menu = None;
        if entry.is_dir {
            self.open_dir(entry.path.clone(), cx);
        } else if ev.click_count() >= 2 && !self.files.remote {
            cx.open_with_system(std::path::Path::new(&entry.path));
        } else {
            self.files.selected = Some(entry.path.clone());
            cx.notify();
        }
    }

    fn copy_path(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        self.files.selected = Some(path.to_string());
        self.files.notice = Some("Ruta copiada".into());
        cx.notify();
    }

    fn open_file_menu(&mut self, entry: FileEntry, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.files.selected = Some(entry.path.clone());
        self.files.menu = Some(FileMenu { entry, position });
        cx.notify();
    }

    fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        if self.files.menu.take().is_some() {
            cx.notify();
        }
    }

    // ── Transferencias ──────────────────────────────────────────────────────

    /// Clic derecho → "Descargar…": elige una carpeta local y encola la descarga.
    fn download(&mut self, entry: FileEntry, cx: &mut Context<Self>) {
        self.files.menu = None;
        let Some(fs) = self.files.fs.clone().filter(|_| self.files.remote) else {
            return;
        };
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Descargar aquí".into()),
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

    /// Archivos soltados desde el Explorador sobre `dest` (o la carpeta mostrada): subir (remoto) o
    /// copiar (local).
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

    /// Botón del agente: elegir archivos con el diálogo de Windows.
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
                e.value.view.read(cx).paste_text(&text);
            }
            self.files.notice = Some(format!("Rutas para el agente en {} (también copiadas)", g.dest_dir));
        }
    }

    /// Repinta la cola mientras haya algo en curso; al terminar un trabajo relee la carpeta.
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
                        w.load_files(cx);
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

    // ── Borrar ──────────────────────────────────────────────────────────────

    fn delete_confirmed(&mut self, cx: &mut Context<Self>) {
        let (Some(entry), Some(fs)) = (self.files.confirm_delete.take(), self.files.fs.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let path = entry.path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { fs.remove_all(&path) })
                .await;
            let _ = this.update(cx, |w, cx| {
                w.files.notice = Some(match result {
                    Ok(()) => format!("Se borró «{}»", entry.name),
                    Err(e) => format!("No se pudo borrar «{}»: {e}", entry.name),
                });
                w.load_files(cx);
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
            // Dentro de la carpeta del proyecto sintético.
            self.files.dir = Some("/home/demo/proyecto".into());
            self.load_files(cx);
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
        let body: AnyElement = match &self.files.listing {
            None => div().flex_1().into_any_element(),
            Some(Err(_)) => {
                empty_note(c, "No se pudo leer la carpeta", "Puede faltar permiso o ya no existir").into_any_element()
            }
            Some(Ok(l)) if l.entries.is_empty() => empty_note(
                c,
                "Carpeta vacía",
                if self.files.remote {
                    "Suelta archivos aquí para subirlos"
                } else {
                    "Suelta archivos aquí para copiarlos"
                },
            )
            .into_any_element(),
            Some(Ok(l)) => {
                let count = l.entries.len();
                let truncated = l.truncated;
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
                        .px(px(6.))
                        .py(px(4.)),
                    )
                    .when(truncated, |this| {
                        this.child(
                            div()
                                .px(px(14.))
                                .py(px(6.))
                                .text_size(px(11.))
                                .text_color(c.muted)
                                .child(format!("Se muestran los primeros {count} elementos")),
                        )
                    })
                    .into_any_element()
            }
        };
        // Soltar sobre la lista (fuera de una carpeta) = a la carpeta mostrada.
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
            .child(body);

        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_files_header(Some(&dir), &tag, cx))
            .child(body)
            .children(self.render_delete_confirm(cx))
            .children(self.render_agent_zone(cx))
            .child(self.render_transfers(cx))
            .children(self.render_files_notice())
            .children(self.render_file_menu(cx))
            .into_any_element()
    }

    /// Encabezado: ↑, nombre de la carpeta y, debajo, la etiqueta local/remoto y la ruta completa.
    fn render_files_header(&self, dir: Option<&str>, tag: &str, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let fs = self.files.fs.as_ref();
        let name = dir
            .map(|d| fs.and_then(|f| f.file_name(d)).unwrap_or_else(|| d.to_string()))
            .unwrap_or_else(|| "Archivos".into());
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
            .gap(px(4.))
            .items_center()
            .child(
                icon_button("files-up", IconName::ArrowUp, c, "Subir a la carpeta superior")
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
                    icon_button(
                        "files-home",
                        IconName::SquareTerminal,
                        c,
                        "Volver a la carpeta de la sesión",
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.files.follow.clone() {
                            this.open_dir(f, cx);
                        }
                    })),
                )
            })
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

    fn file_rows(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = &self.colors;
        let Some(Ok(listing)) = &self.files.listing else {
            return Vec::new();
        };
        let now = now_secs() as i64;
        let tz = jiff::tz::TimeZone::system();
        let (hover, accent) = (c.hover, c.accent);
        listing.entries[range.start.min(listing.entries.len())..range.end.min(listing.entries.len())]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = self.files.selected.as_ref() == Some(&entry.path);
                let date = entry.modified.map(|t| format_modified(t, now, &tz)).unwrap_or_default();
                let size = (!entry.is_dir).then(|| format_size(entry.size));
                let (click, right, target) = (entry.clone(), entry.clone(), entry.path.clone());
                h_flex()
                    .id(("file", range.start + i))
                    // Sin ancho completo, el nombre no se estira y la fecha queda pegada a él.
                    .w_full()
                    .h(px(ROW))
                    .px(px(8.))
                    .gap(px(8.))
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .map(|this| {
                        if selected {
                            this.bg(c.accent.opacity(0.16))
                        } else {
                            this.hover(move |s| s.bg(hover))
                        }
                    })
                    // Soltar sobre una carpeta = a esa carpeta.
                    .when(entry.is_dir, |this| {
                        this.drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(accent.opacity(0.24)))
                            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                                this.drop_paths(paths.paths(), Some(target.clone()), cx)
                            }))
                    })
                    .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.on_file_click(&click, ev, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            this.open_file_menu(right.clone(), ev.position, cx)
                        }),
                    )
                    .child(
                        Icon::new(if entry.is_dir { IconName::Folder } else { IconName::File })
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(c.muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(c.text)
                            .child(entry.name.clone()),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .when_some(size, |this, s| this.child(s))
                            // Cabe "abr 2024" (fechas de otro año), la más ancha de `format_modified`.
                            .child(div().w(px(DATE_COL)).flex().justify_end().child(date)),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// Menú del clic derecho.
    fn render_file_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.files.menu.as_ref()?;
        let c = &self.colors;
        let entry = menu.entry.clone();
        let hover = c.hover;
        let item = |id: &'static str, icon: IconName, label: &'static str| {
            h_flex()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .gap(px(8.))
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(Icon::new(icon).size(px(14.)).text_color(c.muted))
                .child(label)
        };
        let remote = self.files.remote;
        let (e_down, e_open, e_copy, e_del) = (entry.clone(), entry.clone(), entry.clone(), entry);
        let list = v_flex()
            .w(px(MENU_W))
            .p(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(c.border)
            .bg(c.popover)
            .shadow_lg()
            .text_size(px(12.5))
            .text_color(c.text)
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_file_menu(cx)))
            .when(remote, |this| {
                this.child(
                    item("menu-download", IconName::Download, "Descargar…")
                        .on_click(cx.listener(move |this, _, _, cx| this.download(e_down.clone(), cx))),
                )
            })
            .when(!remote && !e_open.is_dir, |this| {
                this.child(item("menu-open", IconName::ExternalLink, "Abrir").on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.files.menu = None;
                        cx.open_with_system(std::path::Path::new(&e_open.path));
                        cx.notify();
                    },
                )))
            })
            .child(
                item("menu-copy", IconName::Copy, "Copiar ruta").on_click(cx.listener(move |this, _, _, cx| {
                    this.files.menu = None;
                    this.copy_path(&e_copy.path, cx);
                })),
            )
            .child(div().h(px(1.)).my(px(3.)).bg(c.border))
            .child(
                item("menu-delete", IconName::Trash, "Borrar…")
                    .text_color(c.error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.files.menu = None;
                        this.files.confirm_delete = Some(e_del.clone());
                        cx.notify();
                    })),
            );
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// "¿Borrar «x»?" al pie del árbol.
    fn render_delete_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let entry = self.files.confirm_delete.as_ref()?;
        let c = &self.colors;
        let what = if entry.is_dir {
            format!("¿Borrar la carpeta «{}» y todo su contenido?", entry.name)
        } else {
            format!("¿Borrar «{}»?", entry.name)
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
                .child(div().text_color(c.text).child(what))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(c.muted)
                        .child(if self.files.remote {
                            "Se borra en el servidor. No se puede deshacer."
                        } else {
                            "No va a la papelera. No se puede deshacer."
                        }),
                )
                .child(
                    h_flex()
                        .gap(px(6.))
                        .justify_end()
                        .child(
                            small_button("delete-cancel", "Cancelar", c.text, c).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.files.confirm_delete = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            small_button("delete-ok", "Borrar", c.error, c)
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

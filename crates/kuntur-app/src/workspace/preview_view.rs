//! Vista previa (F8, especificación §9 y §14): divide la zona central —terminal a la izquierda,
//! vista previa a la derecha— con pestañas para varios archivos. Formatos: Markdown, imágenes,
//! texto/código con números de línea, diff a colores, y PDF/HTML (y Office convertido a PDF con
//! LibreOffice) en un WebView2 (`wry`) que se crea al abrir una pestaña web y se destruye al cerrar
//! la última. Los archivos abiertos se recargan solos (`notify`).
//!
//! La lógica (tipo de archivo, lectura, diff, vigilancia) vive en `kuntur-preview`, con pruebas.
//! El resultado del spike de `wry` está en `docs/plans/f8-spike-wry.md`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::text::TextView;
use kuntur_preview::{
    Content, DiffLine, DiffLineKind, DiffStats, FileKind, FileWatcher, diff_texts, git_diff, parse_unified, same_file,
};
use kuntur_term::Link;

use super::*;
use crate::terminal_view::LinkEvent;

const SASH: f32 = 5.0;
const TAB_BAR: f32 = 32.0;
const TOOLBAR: f32 = 30.0;
/// Ancho de la vista previa como fracción del área central.
const DEFAULT_RATIO: f32 = 0.45;
const MIN_RATIO: f32 = 0.2;
const MAX_RATIO: f32 = 0.8;
/// Espera para juntar la ráfaga de avisos de un guardado (temporal + renombrar).
const RELOAD_DELAY: Duration = Duration::from_millis(150);
/// Imágenes más grandes no se cargan en memoria (se ofrece abrirlas con su programa).
const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;
const MENU_WIDTH: f32 = 230.0;

/// Proporción del ancho de la vista previa según la posición del mouse al arrastrar el separador
/// (la vista previa está a la derecha). Valores rotos → la de siempre.
pub(super) fn ratio_for_pointer(origin_x: f32, width: f32, pointer_x: f32) -> f32 {
    if !(width.is_finite() && width > 1.0 && pointer_x.is_finite()) {
        return DEFAULT_RATIO;
    }
    ((origin_x + width - pointer_x) / width).clamp(MIN_RATIO, MAX_RATIO)
}

/// Valor que viaja mientras se arrastra el separador de la vista previa.
pub(super) struct PreviewDrag;

impl Render for PreviewDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Menú contextual de un enlace (clic derecho en la terminal).
pub(super) struct LinkMenu {
    link: Link,
    /// Posición del clic en la ventana.
    pos: Point<Pixels>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Preview,
    OpenWithSystem,
    Reveal,
    Copy,
    OpenUrl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffSource {
    /// `git diff` del archivo contra HEAD.
    Git,
    /// Contra la versión que se vio antes de la última recarga.
    Previous,
}

struct DiffView {
    source: Option<DiffSource>,
    lines: Rc<Vec<DiffLine>>,
    stats: DiffStats,
}

/// Lo que se leyó en segundo plano.
struct LoadResult {
    kind: FileKind,
    content: Content,
    image: Option<(ImageFormat, Vec<u8>)>,
    /// Archivo a mostrar en el visor web (el mismo PDF/HTML o el PDF convertido desde Office).
    web_file: Option<PathBuf>,
}

enum TabState {
    Loading,
    Error(String),
    Ready {
        content: Content,
        /// Líneas del texto, listas para la lista virtual.
        lines: Rc<Vec<SharedString>>,
        image: Option<Arc<Image>>,
    },
}

struct PreviewTab {
    path: PathBuf,
    kind: FileKind,
    state: TabState,
    /// Identificador único (ids de elementos y descartar cargas viejas).
    id: u64,
    /// Cuántas veces se recargó (el Markdown se vuelve a parsear con un id nuevo).
    version: u64,
    /// Línea pedida por el enlace (`archivo:12`).
    line: Option<u32>,
    diff_mode: bool,
    diff: Option<DiffView>,
    /// Texto que se vio antes de la última recarga (diff "contra la versión anterior").
    previous: Option<String>,
    web_file: Option<PathBuf>,
    /// PDF temporal convertido con LibreOffice (se borra al cerrar la pestaña).
    temp_pdf: Option<PathBuf>,
}

/// El WebView2 de la ventana (uno solo, compartido por las pestañas PDF/HTML).
struct WebHost {
    view: wry::WebView,
    _context: wry::WebContext,
    url: String,
    visible: bool,
    bounds: Bounds<Pixels>,
}

#[derive(Default)]
pub(super) struct PreviewState {
    tabs: Vec<PreviewTab>,
    active: usize,
    ratio: Option<f32>,
    link_menu: Option<LinkMenu>,
    watcher: Option<FileWatcher>,
    _watch_task: Option<Task<()>>,
    web: Rc<RefCell<Option<WebHost>>>,
    next_id: u64,
    /// LibreOffice: `None` = aún no se buscó.
    libreoffice: Option<Option<PathBuf>>,
    /// Prueba de humo: las pestañas nuevas abren en modo diff.
    smoke_diff: bool,
}

impl PreviewState {
    fn active_tab(&self) -> Option<&PreviewTab> {
        self.tabs.get(self.active)
    }

    fn ratio(&self) -> f32 {
        self.ratio.unwrap_or(DEFAULT_RATIO)
    }

    /// Devuelve el teclado a la ventana GPUI si lo tenía el WebView2.
    fn focus_parent(&self) {
        if let Some(h) = self.web.borrow().as_ref() {
            let _ = h.view.focus_parent();
        }
    }
}

fn image_format(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        "webp" => ImageFormat::Webp,
        "bmp" => ImageFormat::Bmp,
        "ico" => ImageFormat::Ico,
        "svg" => ImageFormat::Svg,
        _ => return None,
    })
}

/// Lee el archivo (en segundo plano). Office → PDF solo si LibreOffice está instalado.
fn load_file(path: &Path, libreoffice: Option<&Path>, temp_dir: &Path) -> Result<LoadResult, String> {
    let (kind, content) = kuntur_preview::load(path).map_err(|e| e.to_string())?;
    let mut image = None;
    let mut web_file = None;
    match kind {
        FileKind::Image | FileKind::Svg => {
            let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
            if size > MAX_IMAGE_BYTES {
                return Err(format!("Imagen demasiado grande ({} MB)", size / (1024 * 1024)));
            }
            let format = image_format(path).ok_or("Formato de imagen no reconocido")?;
            image = Some((format, std::fs::read(path).map_err(|e| e.to_string())?));
        }
        FileKind::Pdf | FileKind::Html => web_file = Some(path.to_path_buf()),
        FileKind::Office => {
            if let Some(soffice) = libreoffice {
                web_file = kuntur_preview::convert_to_pdf(soffice, path, temp_dir).ok();
            }
        }
        _ => {}
    }
    Ok(LoadResult {
        kind,
        content,
        image,
        web_file,
    })
}

/// Carpeta de datos del WebView2 (si no, crea `kuntur.exe.WebView2` junto al ejecutable).
fn webview_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Kuntur").join("WebView2"))
}

fn web_rect(b: Bounds<Pixels>) -> wry::Rect {
    use wry::dpi::{LogicalPosition, LogicalSize};
    wry::Rect {
        position: LogicalPosition::new(f32::from(b.origin.x) as f64, f32::from(b.origin.y) as f64).into(),
        size: LogicalSize::new(
            f32::from(b.size.width).max(1.0) as f64,
            f32::from(b.size.height).max(1.0) as f64,
        )
        .into(),
    }
}

impl Workspace {
    // ── Enlaces de la terminal ──────────────────────────────────────────────

    /// Escucha los enlaces de una terminal (principal o acoplada). La suscripción vive lo que la
    /// terminal.
    pub(super) fn watch_links(&self, view: &Entity<TerminalView>, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(view, window, |this, _, ev: &LinkEvent, window, cx| {
            this.on_link(ev.clone(), window, cx)
        })
        .detach();
    }

    fn on_link(&mut self, ev: LinkEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            LinkEvent::Open(link) => self.open_link(link, window, cx),
            LinkEvent::Menu(link, pos) => {
                self.preview.link_menu = Some(LinkMenu { link, pos });
                cx.notify();
            }
        }
    }

    /// Ctrl+clic: URL → navegador; carpeta → Explorador; archivo → vista previa.
    fn open_link(&mut self, link: Link, window: &mut Window, cx: &mut Context<Self>) {
        match link {
            Link::Url(url) => cx.open_url(&url),
            Link::File { path, .. } if path.is_dir() => cx.open_with_system(&path),
            Link::File { path, line, .. } => self.open_preview(path, line, window, cx),
        }
    }

    fn run_menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.preview.link_menu.take() else {
            return;
        };
        match (action, menu.link) {
            (MenuAction::Copy, link) => cx.write_to_clipboard(ClipboardItem::new_string(link.display())),
            (MenuAction::OpenUrl, Link::Url(url)) => cx.open_url(&url),
            (MenuAction::Preview, Link::File { path, line, .. }) => self.open_preview(path, line, window, cx),
            (MenuAction::OpenWithSystem, Link::File { path, .. }) => cx.open_with_system(&path),
            (MenuAction::Reveal, Link::File { path, .. }) => cx.reveal_path(&path),
            _ => {}
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    // ── Pestañas ────────────────────────────────────────────────────────────

    /// Abre el archivo en la vista previa (o activa su pestaña si ya estaba abierta).
    pub(super) fn open_preview(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self.preview.tabs.iter().position(|t| same_file(&t.path, &path)) {
            self.preview.active = i;
            self.preview.tabs[i].line = line.or(self.preview.tabs[i].line);
            self.sync_web(window, cx);
            cx.notify();
            return;
        }
        self.preview.next_id += 1;
        let id = self.preview.next_id;
        self.preview.tabs.push(PreviewTab {
            kind: kuntur_preview::kind_for(&path, b""),
            path: path.clone(),
            state: TabState::Loading,
            id,
            version: 0,
            line,
            diff_mode: self.preview.smoke_diff,
            diff: None,
            previous: None,
            web_file: None,
            temp_pdf: None,
        });
        self.preview.active = self.preview.tabs.len() - 1;
        self.ensure_watcher(window, cx);
        if let Some(w) = self.preview.watcher.as_mut() {
            let _ = w.watch(&path);
        }
        self.start_load(id, window, cx);
        cx.notify();
    }

    fn tab_index(&self, id: u64) -> Option<usize> {
        self.preview.tabs.iter().position(|t| t.id == id)
    }

    /// Lee (o vuelve a leer) el archivo de la pestaña en segundo plano.
    fn start_load(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let path = self.preview.tabs[i].path.clone();
        let libreoffice = match kuntur_preview::kind_for(&path, b"") {
            FileKind::Office => self
                .preview
                .libreoffice
                .get_or_insert_with(kuntur_preview::find_libreoffice)
                .clone(),
            _ => None,
        };
        let temp_dir = std::env::temp_dir().join("kuntur-preview").join(id.to_string());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_file(&path, libreoffice.as_deref(), &temp_dir) })
                .await;
            let _ = this.update_in(cx, |w, window, cx| w.apply_load(id, result, window, cx));
        })
        .detach();
    }

    fn apply_load(&mut self, id: u64, result: Result<LoadResult, String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.preview.tabs[i];
        // El texto que se veía pasa a ser "la versión anterior" si cambió.
        let old_text = match &tab.state {
            TabState::Ready { content, .. } => content.text().map(str::to_owned),
            _ => None,
        };
        tab.version += 1;
        match result {
            Ok(r) => {
                if let (Some(old), Some(new)) = (old_text, r.content.text())
                    && old != new
                {
                    tab.previous = Some(old);
                }
                let lines = r
                    .content
                    .text()
                    .map(|t| t.lines().map(|l| SharedString::from(l.replace('\t', "    "))).collect())
                    .unwrap_or_default();
                let image = r
                    .image
                    .map(|(format, bytes)| Arc::new(Image::from_bytes(format, bytes)));
                if r.web_file.as_ref() != Some(&tab.path) && r.web_file.is_some() {
                    tab.temp_pdf = r.web_file.clone();
                }
                tab.kind = r.kind;
                tab.web_file = r.web_file;
                tab.state = TabState::Ready {
                    content: r.content,
                    lines: Rc::new(lines),
                    image,
                };
            }
            Err(e) => tab.state = TabState::Error(e),
        }
        if tab.diff_mode {
            self.compute_diff(id, window, cx);
        }
        self.sync_web(window, cx);
        cx.notify();
    }

    fn activate_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i < self.preview.tabs.len() {
            self.preview.active = i;
            self.sync_web(window, cx);
            cx.notify();
        }
    }

    fn close_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i >= self.preview.tabs.len() {
            return;
        }
        let tab = self.preview.tabs.remove(i);
        if let Some(w) = self.preview.watcher.as_mut() {
            w.unwatch(&tab.path);
        }
        if let Some(pdf) = &tab.temp_pdf {
            let _ = std::fs::remove_file(pdf);
        }
        if self.preview.active >= i && self.preview.active > 0 {
            self.preview.active -= 1;
        }
        if self.preview.tabs.is_empty() {
            self.preview.watcher = None;
            self.preview._watch_task = None;
            self.focus_active(window, cx);
        }
        self.sync_web(window, cx);
        cx.notify();
    }

    fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while !self.preview.tabs.is_empty() {
            self.close_tab(self.preview.tabs.len() - 1, window, cx);
        }
    }

    // ── Diff ────────────────────────────────────────────────────────────────

    fn toggle_diff(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.preview.tabs.get_mut(i) else {
            return;
        };
        tab.diff_mode = !tab.diff_mode;
        let id = tab.id;
        if tab.diff_mode {
            self.compute_diff(id, window, cx);
        }
        cx.notify();
    }

    /// `git diff` del archivo; si no hay cambios en git, contra la versión que se vio antes.
    fn compute_diff(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &self.preview.tabs[i];
        let path = tab.path.clone();
        let current = match &tab.state {
            TabState::Ready { content, .. } => content.text().map(str::to_owned),
            _ => None,
        };
        let previous = tab.previous.clone();
        cx.spawn_in(window, async move |this, cx| {
            let view = cx
                .background_executor()
                .spawn(async move {
                    match git_diff(&path) {
                        Some(out) => (Some(DiffSource::Git), parse_unified(&out)),
                        None => match (previous, current) {
                            (Some(old), Some(new)) => (Some(DiffSource::Previous), diff_texts(&old, &new, 3)),
                            _ => (None, Vec::new()),
                        },
                    }
                })
                .await;
            let _ = this.update(cx, |w, cx| {
                if let Some(i) = w.tab_index(id) {
                    let (source, lines) = view;
                    w.preview.tabs[i].diff = Some(DiffView {
                        source: source.filter(|_| !lines.is_empty()),
                        stats: DiffStats::of(&lines),
                        lines: Rc::new(lines),
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ── Recarga automática ──────────────────────────────────────────────────

    /// Crea el vigilante la primera vez: los avisos llegan por un canal y se juntan durante
    /// `RELOAD_DELAY` antes de recargar.
    fn ensure_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.watcher.is_some() {
            return;
        }
        let (tx, rx) = smol::channel::unbounded::<PathBuf>();
        let Ok(watcher) = FileWatcher::new(move |p| {
            let _ = tx.try_send(p);
        }) else {
            return;
        };
        self.preview.watcher = Some(watcher);
        self.preview._watch_task = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(first) = rx.recv().await {
                cx.background_executor().timer(RELOAD_DELAY).await;
                let mut changed = vec![first];
                while let Ok(p) = rx.try_recv() {
                    changed.push(p);
                }
                if this
                    .update_in(cx, |w, window, cx| w.files_changed(&changed, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// Algún archivo de una carpeta vigilada cambió: recargar las pestañas afectadas.
    fn files_changed(&mut self, changed: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .preview
            .tabs
            .iter()
            .filter(|t| changed.iter().any(|p| same_file(p, &t.path)))
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.start_load(id, window, cx);
        }
    }

    // ── Visor web (WebView2) ────────────────────────────────────────────────

    /// Crea, navega o destruye el WebView2 según las pestañas: existe solo mientras haya alguna
    /// pestaña PDF/HTML; muestra la activa si es web.
    fn sync_web(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        let any_web = self.preview.tabs.iter().any(|t| t.web_file.is_some());
        if !any_web {
            // Soltarlo destruye la ventana hija y sus procesos.
            self.preview.web.borrow_mut().take();
            return;
        }
        let Some(file) = self.preview.active_tab().and_then(|t| t.web_file.clone()) else {
            return;
        };
        let url = kuntur_preview::file_url(&file);
        let mut web = self.preview.web.borrow_mut();
        match web.as_mut() {
            Some(h) if h.url == url => {}
            Some(h) => {
                let _ = h.view.load_url(&url);
                h.url = url;
            }
            None => {
                let mut context = wry::WebContext::new(webview_data_dir());
                let built = wry::WebViewBuilder::new_with_web_context(&mut context)
                    .with_url(&url)
                    .with_visible(false)
                    .with_focused(false)
                    .build_as_child(&*window);
                if let Ok(view) = built {
                    *web = Some(WebHost {
                        view,
                        _context: context,
                        url,
                        visible: false,
                        bounds: Bounds::default(),
                    });
                }
            }
        }
    }

    /// Cada cuadro: el WebView2 se ve solo si la pestaña activa es web y nada de Kuntur tiene que
    /// dibujarse encima (paleta, menú de enlaces, vista de uso): una ventana hija siempre queda por
    /// encima de lo que dibuja GPUI.
    pub(super) fn sync_web_visibility(&self) {
        let mut web = self.preview.web.borrow_mut();
        let Some(h) = web.as_mut() else {
            return;
        };
        let want = !self.usage.open
            && self.palette.is_none()
            && self.preview.link_menu.is_none()
            && self.preview.active_tab().is_some_and(|t| t.web_file.is_some())
            && h.bounds.size.width > px(1.)
            && h.bounds.size.height > px(1.);
        if want != h.visible {
            h.visible = want;
            let _ = h.view.set_visible(want);
        }
    }

    // ── Dibujo ──────────────────────────────────────────────────────────────

    /// Zona central: las terminales solas o, con pestañas abiertas, terminales | separador |
    /// vista previa.
    pub(super) fn render_with_preview(&self, terminals: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        if self.preview.tabs.is_empty() {
            return terminals;
        }
        let c = &self.colors;
        let (border, accent) = (c.border, c.accent);
        let ratio = self.preview.ratio();
        let sash = div()
            .id("preview-sash")
            .w(px(SASH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .cursor_ew_resize()
            .bg(c.terminal)
            .hover(move |s| s.bg(accent.opacity(0.3)))
            .on_drag(PreviewDrag, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| PreviewDrag)
            })
            .child(div().w(px(1.)).h_full().bg(border));

        h_flex()
            .id("preview-split")
            .size_full()
            .on_drag_move(cx.listener(|this, ev: &DragMoveEvent<PreviewDrag>, _, cx| {
                let b = ev.bounds;
                this.preview.ratio = Some(ratio_for_pointer(
                    f32::from(b.origin.x),
                    f32::from(b.size.width),
                    f32::from(ev.event.position.x),
                ));
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    // Un clic en la terminal le devuelve el teclado a la ventana si lo tenía el visor web.
                    .on_any_mouse_down(cx.listener(|this, _, _, _| this.preview.focus_parent()))
                    .child(terminals),
            )
            .child(sash)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .w(relative(ratio))
                    .h_full()
                    .min_w_0()
                    .bg(c.terminal)
                    .child(self.render_preview_tabs(cx))
                    .child(self.render_preview_toolbar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .overflow_hidden()
                            .child(self.render_preview_body()),
                    ),
            )
            .into_any_element()
    }

    fn render_preview_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let hover = c.hover;
        let tabs: Vec<AnyElement> = self
            .preview
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let active = i == self.preview.active;
                let name = t
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let full: SharedString = t.path.display().to_string().into();
                h_flex()
                    .id(("preview-tab", t.id))
                    .h_full()
                    .max_w(px(200.))
                    .pl(px(10.))
                    .pr(px(4.))
                    .gap(px(4.))
                    .items_center()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(c.border)
                    .cursor_pointer()
                    .text_color(if active { c.text } else { c.muted })
                    .when(active, |d| d.bg(c.window).border_b_2().border_color(c.accent))
                    .when(!active, |d| d.hover(move |s| s.bg(hover)))
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| this.activate_tab(i, window, cx)))
                    .child(
                        div()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(
                        div()
                            .id(("preview-tab-close", t.id))
                            .size(px(18.))
                            .flex_shrink_0()
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |s| s.bg(hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_tab(i, window, cx);
                            }))
                            .child(Icon::new(IconName::Close).size(px(11.))),
                    )
                    .into_any_element()
            })
            .collect();
        h_flex()
            .id("preview-tabs")
            .h(px(TAB_BAR))
            .flex_shrink_0()
            .w_full()
            .overflow_x_scroll()
            .border_b_1()
            .border_color(c.border)
            .text_size(px(12.))
            .children(tabs)
            .into_any_element()
    }

    fn render_preview_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = self.colors.clone();
        let Some(tab) = self.preview.active_tab() else {
            return div().into_any_element();
        };
        let i = self.preview.active;
        let path = tab.path.clone();
        let shown = kuntur_files::display_path(&path, self.home.as_deref());
        let textual = tab.kind.is_textual();
        let truncated = matches!(&tab.state, TabState::Ready { content: Content::Text(d) | Content::Markdown(d), .. } if d.truncated);
        let (accent, hover) = (c.accent, c.hover);

        let diff_label = match tab.diff.as_ref().filter(|_| tab.diff_mode) {
            Some(d) if d.source.is_some() => format!("Cambios +{} −{}", d.stats.added, d.stats.removed),
            _ => "Cambios".to_string(),
        };
        let diff_button = textual.then(|| {
            div()
                .id("preview-diff")
                .px(px(8.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(if tab.diff_mode { c.text } else { c.muted })
                .when(tab.diff_mode, |d| d.bg(accent.opacity(0.22)))
                .hover(move |s| s.bg(hover))
                .tooltip(|window, cx| Tooltip::new("Ver qué cambió (git diff o la versión anterior)").build(window, cx))
                .on_click(cx.listener(move |this, _, window, cx| this.toggle_diff(i, window, cx)))
                .child(diff_label)
        });

        let (p1, p2) = (path.clone(), path.clone());
        h_flex()
            .h(px(TOOLBAR))
            .flex_shrink_0()
            .w_full()
            .pl(px(10.))
            .pr(px(4.))
            .gap(px(4.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
            .text_size(px(11.5))
            .text_color(c.muted)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(shown),
            )
            .when(truncated, |d| {
                d.child(div().flex_shrink_0().child("(solo el comienzo)"))
            })
            .children(diff_button)
            .child(
                icon_button("preview-open", IconName::ExternalLink, &c, "Abrir con su programa")
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_with_system(&p1))),
            )
            .child(
                icon_button("preview-reveal", IconName::FolderOpen, &c, "Mostrar en carpeta")
                    .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&p2))),
            )
            .child(
                icon_button("preview-close-all", IconName::Close, &c, "Cerrar la vista previa")
                    .on_click(cx.listener(|this, _, window, cx| this.close_all_tabs(window, cx))),
            )
            .into_any_element()
    }

    fn render_preview_body(&self) -> AnyElement {
        let c = self.colors.clone();
        let Some(tab) = self.preview.active_tab() else {
            return div().into_any_element();
        };
        let message = |text: String, c: &UiColors| {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .p(px(20.))
                .text_color(c.muted)
                .text_center()
                .child(text)
                .into_any_element()
        };
        if tab.diff_mode && tab.kind.is_textual() {
            return match &tab.diff {
                None => message("Calculando cambios…".into(), &c),
                Some(d) if d.source.is_none() => message(
                    "Sin cambios: el archivo no tiene cambios en git y no cambió desde que se abrió.".into(),
                    &c,
                ),
                Some(d) => self.render_diff(tab.id, d, &c),
            };
        }
        match &tab.state {
            TabState::Loading => message("Cargando…".into(), &c),
            TabState::Error(e) => message(format!("No se pudo leer el archivo: {e}"), &c),
            TabState::Ready { content, lines, image } => match content {
                Content::Markdown(doc) => div()
                    .size_full()
                    .p(px(16.))
                    .text_size(px(13.5))
                    .child(
                        TextView::markdown(("preview-md", tab.id * 1_000_000 + tab.version), doc.text.clone())
                            .selectable(true)
                            .scrollable(true)
                            .size_full(),
                    )
                    .into_any_element(),
                Content::Text(_) => self.render_text_lines(tab.id, lines.clone(), tab.line, &c),
                Content::Image => match image {
                    Some(image) => div()
                        .size_full()
                        .p(px(12.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(img(image.clone()).max_w_full().max_h_full().object_fit(ObjectFit::Contain))
                        .into_any_element(),
                    None => message("Imagen no disponible".into(), &c),
                },
                Content::Web | Content::Office if tab.web_file.is_some() => {
                    // El WebView2 se coloca sobre este rectángulo (en píxeles lógicos de la ventana).
                    let web = self.preview.web.clone();
                    div()
                        .size_full()
                        .child(
                            canvas(
                                move |bounds, _, _| {
                                    if let Some(h) = web.borrow_mut().as_mut()
                                        && h.bounds != bounds
                                    {
                                        h.bounds = bounds;
                                        let _ = h.view.set_bounds(web_rect(bounds));
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
                        )
                        .into_any_element()
                }
                Content::Office => message(
                    "Para ver documentos de Office aquí hace falta LibreOffice (no está instalado). Usa «Abrir con su programa» (arriba).".into(),
                    &c,
                ),
                Content::Web => message("No se pudo abrir el visor web. Usa «Abrir con su programa» (arriba).".into(), &c),
                Content::Binary { size } => message(
                    format!(
                        "Archivo binario ({}). Usa «Abrir con su programa» (arriba).",
                        human_size(*size)
                    ),
                    &c,
                ),
            },
        }
    }

    /// Texto con números de línea (lista virtual: solo se dibujan las filas visibles).
    fn render_text_lines(
        &self,
        id: u64,
        lines: Rc<Vec<SharedString>>,
        target: Option<u32>,
        c: &UiColors,
    ) -> AnyElement {
        let digits = lines.len().max(1).to_string().len();
        let (muted, text, accent) = (c.muted, c.text, c.accent);
        let mono = self.mono_font.clone();
        let size = px((self.settings.font_size - 1.0).max(9.0));
        let row_h = (size * 1.45).round();
        let count = lines.len();
        uniform_list(("preview-text", id), count, move |range, _, _| {
            range
                .map(|i| {
                    let marked = target == Some(i as u32 + 1);
                    h_flex()
                        .h(row_h)
                        .w_full()
                        .overflow_hidden()
                        .px(px(8.))
                        .gap(px(12.))
                        .font_family(mono.clone())
                        .text_size(size)
                        .whitespace_nowrap()
                        .when(marked, |d| d.bg(accent.opacity(0.18)))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(size * (digits as f32 * 0.62 + 0.3))
                                .text_right()
                                .text_color(muted)
                                .child(format!("{}", i + 1)),
                        )
                        .child(div().text_color(text).child(lines[i].clone()))
                })
                .collect()
        })
        .size_full()
        .into_any_element()
    }

    /// Diff a colores: verde las agregadas, rojo las quitadas, números de línea viejo | nuevo.
    fn render_diff(&self, id: u64, d: &DiffView, c: &UiColors) -> AnyElement {
        let lines = d.lines.clone();
        let (muted, text, accent) = (c.muted, c.text, c.accent);
        let added: Hsla = rgb(0x3fb950).into();
        let removed: Hsla = rgb(0xf85149).into();
        let mono = self.mono_font.clone();
        let size = px((self.settings.font_size - 1.0).max(9.0));
        let row_h = (size * 1.45).round();
        let source = match d.source {
            Some(DiffSource::Git) => "git diff (contra HEAD)",
            _ => "contra la versión anterior que se vio",
        };
        let count = lines.len();
        let list = uniform_list(("preview-diff", id), count, move |range, _, _| {
            range
                .map(|i| {
                    let l = &lines[i];
                    let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
                    let (bg, fg, sign) = match l.kind {
                        DiffLineKind::Added => (Some(added.opacity(0.16)), text, "+"),
                        DiffLineKind::Removed => (Some(removed.opacity(0.16)), text, "−"),
                        DiffLineKind::Hunk => (Some(accent.opacity(0.10)), accent, ""),
                        DiffLineKind::Meta | DiffLineKind::NoNewline => (None, muted, ""),
                        DiffLineKind::Context => (None, text, " "),
                    };
                    let sign_color = match l.kind {
                        DiffLineKind::Added => added,
                        DiffLineKind::Removed => removed,
                        _ => muted,
                    };
                    h_flex()
                        .h(row_h)
                        .w_full()
                        .overflow_hidden()
                        .px(px(6.))
                        .gap(px(6.))
                        .font_family(mono.clone())
                        .text_size(size)
                        .whitespace_nowrap()
                        .when_some(bg, |d, bg| d.bg(bg))
                        .child(
                            div()
                                .w(size * 2.6)
                                .flex_shrink_0()
                                .text_right()
                                .text_color(muted)
                                .child(num(l.old_no)),
                        )
                        .child(
                            div()
                                .w(size * 2.6)
                                .flex_shrink_0()
                                .text_right()
                                .text_color(muted)
                                .child(num(l.new_no)),
                        )
                        .child(div().w(size * 0.8).flex_shrink_0().text_color(sign_color).child(sign))
                        .child(div().text_color(fg).child(l.text.replace('\t', "    ")))
                })
                .collect()
        })
        .flex_1()
        .min_h_0();
        v_flex()
            .size_full()
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(format!("{source} · +{} −{}", d.stats.added, d.stats.removed)),
            )
            .child(list)
            .into_any_element()
    }

    /// Menú contextual de un enlace, en la posición del clic. Un clic fuera lo cierra.
    pub(super) fn render_link_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.preview.link_menu.as_ref()?;
        let c = self.colors.clone();
        let items: Vec<(MenuAction, IconName, &str)> = match &menu.link {
            Link::Url(_) => vec![
                (MenuAction::OpenUrl, IconName::Globe, "Abrir en el navegador"),
                (MenuAction::Copy, IconName::Copy, "Copiar URL"),
            ],
            Link::File { path, .. } if path.is_dir() => vec![
                (MenuAction::OpenWithSystem, IconName::FolderOpen, "Abrir carpeta"),
                (MenuAction::Copy, IconName::Copy, "Copiar ruta"),
            ],
            Link::File { .. } => vec![
                (MenuAction::Preview, IconName::Eye, "Vista previa"),
                (
                    MenuAction::OpenWithSystem,
                    IconName::ExternalLink,
                    "Abrir con su programa",
                ),
                (MenuAction::Reveal, IconName::FolderOpen, "Mostrar en carpeta"),
                (MenuAction::Copy, IconName::Copy, "Copiar ruta"),
            ],
        };
        let hover = c.hover;
        let rows: Vec<AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(n, (action, icon, label))| {
                h_flex()
                    .id(("link-menu-item", n))
                    .h(px(28.))
                    .px(px(10.))
                    .gap(px(8.))
                    .items_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |this, _, window, cx| this.run_menu_action(action, window, cx)))
                    .child(Icon::new(icon).size(px(14.)).text_color(c.muted))
                    .child(label.to_string())
                    .into_any_element()
            })
            .collect();
        let title = menu.link.display();
        let panel = v_flex()
            .id("link-menu")
            .absolute()
            .left(menu.pos.x)
            .top(menu.pos.y)
            .w(px(MENU_WIDTH))
            .p(px(4.))
            .bg(c.popover)
            .border_1()
            .border_color(c.border)
            .rounded(px(8.))
            .shadow_lg()
            .occlude()
            .text_size(px(12.5))
            .text_color(c.text)
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(title),
            )
            .children(rows);
        Some(
            deferred(
                div()
                    .id("link-menu-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .occlude()
                    .on_any_mouse_down(cx.listener(|this, _, _, cx| {
                        this.preview.link_menu = None;
                        cx.notify();
                    }))
                    .child(panel),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    // ── Prueba de humo ──────────────────────────────────────────────────────

    /// `KUNTUR_SMOKE_PREVIEW=ruta[;ruta…]` abre esos archivos en la vista previa al iniciar
    /// (`KUNTUR_SMOKE_PREVIEW_DIFF=1`: en modo diff). `KUNTUR_SMOKE_LINKS=1` subraya los enlaces
    /// de la primera sesión después de sus comandos y abre el menú del primero que sea una ruta.
    pub(super) fn smoke_preview(&mut self, delay: Duration, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(list) = std::env::var("KUNTUR_SMOKE_PREVIEW") {
            self.config_path = None;
            self.preview.smoke_diff = std::env::var("KUNTUR_SMOKE_PREVIEW_DIFF").is_ok();
            let base = std::env::current_dir().unwrap_or_default();
            for p in list.split(';').filter(|p| !p.trim().is_empty()) {
                self.open_preview(base.join(p.trim()), None, window, cx);
            }
        }
        if std::env::var("KUNTUR_SMOKE_LINKS").is_ok() {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update_in(cx, |w, window, cx| {
                    if let Some(first) = w.sessions.iter().next().map(|e| e.value.view.clone()) {
                        first.update(cx, |v, cx| v.smoke_links(window, cx));
                    }
                });
            })
            .detach();
        }
    }
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b if b >= 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{b} bytes"),
    }
}

#[cfg(test)]
mod tests {
    // Sin `super::*`: GPUI exporta su propio atributo `test`, que taparía el de Rust.
    use super::{DEFAULT_RATIO, MAX_RATIO, MIN_RATIO, Path, human_size, image_format, ratio_for_pointer};
    use gpui_kit::ImageFormat;

    #[test]
    fn proporcion_del_separador() {
        // Área de 1000 px desde x=100: el mouse en 600 deja la vista previa con la mitad.
        assert!((ratio_for_pointer(100.0, 1000.0, 600.0) - 0.5).abs() < 1e-6);
        // Límites.
        assert_eq!(ratio_for_pointer(0.0, 1000.0, 990.0), MIN_RATIO);
        assert_eq!(ratio_for_pointer(0.0, 1000.0, 10.0), MAX_RATIO);
        // Valores rotos.
        assert_eq!(ratio_for_pointer(0.0, 0.0, 10.0), DEFAULT_RATIO);
        assert_eq!(ratio_for_pointer(0.0, 1000.0, f32::NAN), DEFAULT_RATIO);
    }

    #[test]
    fn tamanos_legibles() {
        assert_eq!(human_size(512), "512 bytes");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn formato_de_imagen_por_extension() {
        assert_eq!(image_format(Path::new("a.PNG")), Some(ImageFormat::Png));
        assert_eq!(image_format(Path::new("a.svg")), Some(ImageFormat::Svg));
        assert_eq!(image_format(Path::new("a.txt")), None);
    }
}

//! Contenido "Archivos" del panel izquierdo: la carpeta actual de la sesión activa (carpetas
//! primero, tamaño y fecha en tenue). Clic en carpeta entra, ↑ sube, doble clic en archivo lo abre
//! con la app de Windows y clic derecho copia la ruta. La lectura corre fuera del hilo de la UI.

use std::ops::Range;
use std::path::{Path, PathBuf};

use kuntur_files::{FileEntry, Listing, format_modified, format_size, list_dir};

use super::*;

const ROW: f32 = 26.0;

#[derive(Default)]
pub(super) struct FilesState {
    /// Carpeta que se muestra (puede ser otra si el usuario navegó).
    dir: Option<PathBuf>,
    /// Carpeta de la sesión la última vez que se sincronizó: si la sesión cambia de carpeta, el
    /// panel la sigue.
    follow: Option<PathBuf>,
    listing: Option<Result<Listing, String>>,
    selected: Option<PathBuf>,
    /// Lectura en curso (se cancela al soltarla).
    task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
}

impl Workspace {
    /// Sigue la carpeta de la sesión activa y vuelve a leer la carpeta mostrada. Solo trabaja con el
    /// panel en "Archivos" y visible; `force` relee aunque nada haya cambiado.
    pub(super) fn sync_files(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.panel != PanelView::Files || self.settings.sidebar != SidebarMode::Full {
            return;
        }
        let cwd = self.active_cwd(cx);
        if cwd != self.files.follow {
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
        let Some(dir) = self.files.dir.clone() else {
            self.files.task = None;
            return;
        };
        let task_dir = dir.clone();
        self.files.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { list_dir(&task_dir).map_err(|e| e.to_string()) })
                .await;
            let _ = this.update(cx, |w, cx| {
                if w.files.dir.as_deref() == Some(dir.as_path()) {
                    if w.files.listing.as_ref() != Some(&result) {
                        w.files.listing = Some(result);
                        cx.notify();
                    }
                }
                w.files.task = None;
            });
        }));
    }

    fn open_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        self.files.dir = Some(dir);
        self.files.selected = None;
        self.files.listing = None;
        self.files.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.load_files(cx);
        cx.notify();
    }

    fn files_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.files.dir.as_deref().and_then(Path::parent) {
            let parent = parent.to_path_buf();
            self.open_dir(parent, cx);
        }
    }

    fn on_file_click(&mut self, entry: &FileEntry, ev: &ClickEvent, cx: &mut Context<Self>) {
        if entry.is_dir {
            self.open_dir(entry.path.clone(), cx);
        } else if ev.click_count() >= 2 {
            cx.open_with_system(&entry.path);
        } else {
            self.files.selected = Some(entry.path.clone());
            cx.notify();
        }
    }

    fn copy_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string_lossy().into_owned()));
        self.files.selected = Some(path.to_path_buf());
        cx.notify();
    }

    pub(super) fn render_files_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.colors;
        let Some(dir) = self.files.dir.clone() else {
            return empty_note(c, "Carpeta desconocida", "Abre un shell PowerShell en la sesión").into_any_element();
        };
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());
        let full = kuntur_files::display_path(&dir, self.home.as_deref());
        let has_parent = dir.parent().is_some();
        let away = self.files.follow.as_ref().is_some_and(|f| f != &dir);

        let header = h_flex()
            .h(px(40.))
            .flex_shrink_0()
            .pl(px(6.))
            .pr(px(6.))
            .gap(px(4.))
            .items_center()
            .border_b_1()
            .border_color(c.border)
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
                    .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                    .child(name),
            )
            .when(away, |this| {
                this.child(
                    icon_button("files-home", IconName::SquareTerminal, c, "Volver a la carpeta de la sesión")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.files.follow.clone() {
                                this.open_dir(f, cx);
                            }
                        })),
                )
            });

        let body: AnyElement = match &self.files.listing {
            None => div().into_any_element(),
            Some(Err(_)) => empty_note(c, "No se pudo leer la carpeta", "Puede faltar permiso o ya no existir")
                .into_any_element(),
            Some(Ok(l)) if l.entries.is_empty() => empty_note(c, "Carpeta vacía", "").into_any_element(),
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

        v_flex().flex_1().min_h_0().child(header).child(body).into_any_element()
    }

    fn file_rows(&self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let c = &self.colors;
        let Some(Ok(listing)) = &self.files.listing else {
            return Vec::new();
        };
        let now = now_secs() as i64;
        let tz = jiff::tz::TimeZone::system();
        let hover = c.hover;
        listing.entries[range.start.min(listing.entries.len())..range.end.min(listing.entries.len())]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = self.files.selected.as_ref() == Some(&entry.path);
                let date = entry.modified.map(|t| format_modified(t, now, &tz)).unwrap_or_default();
                let size = (!entry.is_dir).then(|| format_size(entry.size));
                let (click, right) = (entry.clone(), entry.path.clone());
                h_flex()
                    .id(("file", range.start + i))
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
                    .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.on_file_click(&click, ev, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| this.copy_path(&right, cx)),
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
                            .child(div().w(px(38.)).flex().justify_end().child(date)),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

/// Aviso centrado en el panel: título y, opcional, una línea tenue.
pub(super) fn empty_note(c: &UiColors, title: &'static str, hint: &'static str) -> Div {
    v_flex()
        .flex_1()
        .items_center()
        .pt(px(48.))
        .px(px(20.))
        .gap(px(4.))
        .text_center()
        .child(div().text_size(px(12.5)).text_color(c.text).opacity(0.85).child(title))
        .when(!hint.is_empty(), |this| {
            this.child(div().text_size(px(11.5)).text_color(c.muted).child(hint))
        })
}

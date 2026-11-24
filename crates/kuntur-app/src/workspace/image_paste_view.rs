//! Imágenes como Warp en el `Workspace` (ajustes v0.2): recibe los pedidos de cada terminal
//! ([`PasteEvent`]: Ctrl+V sin texto, archivos soltados, "Imagen…", clic en una miniatura), decide
//! cómo le llega la imagen al agente de esa terminal (`image_paste.rs`) y mantiene al día la barra
//! del agente de cada terminal.
//!
//! - Claude Code local: el agente lee el portapapeles con su tecla (Alt+V en Windows). Si la imagen
//!   vino de un archivo (arrastrar, "Imagen…"), antes se pone en el portapapeles.
//! - Sin agente o un agente sin esa tecla: PNG en la carpeta temporal de Kuntur + su ruta pegada.
//! - Pestaña SSH: el PNG se sube por SFTP a la carpeta "para el agente" y se pega la ruta remota.
//!
//! Siempre queda una copia local (PNG, como mucho 2000 px) para la miniatura y la vista previa.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kuntur_agents::{ImagePaste, ShellFlavor};
use kuntur_files::{FileSystem, PathStyle, agent_dir};
use kuntur_ssh::RemoteSession;

use super::*;
use crate::i18n::{T, t};
use crate::image_paste::{self as ip, Delivery, Target};
use crate::terminal_view::PasteEvent;

/// Pausa entre dos imágenes entregadas con la tecla del agente (cada una pasa por el portapapeles).
const KEY_GAP: Duration = Duration::from_millis(450);

/// De dónde sale una imagen.
enum Source {
    /// Ya está en el portapapeles (Ctrl+V): estos son sus bytes.
    Clipboard(Vec<u8>),
    /// Un archivo (arrastrado, elegido o copiado en el Explorador).
    File(PathBuf),
}

/// A quién le llega lo que se pega en una terminal.
struct PasteTarget {
    /// Nombre del agente que corre ahí y cómo pega imágenes.
    agent: Option<(String, ImagePaste)>,
    remote: Option<Arc<RemoteSession>>,
    /// Carpeta remota "para el agente" de la pestaña.
    remote_dir: String,
    shell: ShellFlavor,
}

impl PasteTarget {
    fn text_target(&self) -> Target {
        match (&self.agent, &self.remote) {
            (_, Some(_)) => Target::Shell(ShellFlavor::Posix),
            (Some(_), None) => Target::Agent,
            (None, None) => Target::Shell(self.shell),
        }
    }
}

/// Una imagen ya preparada y guardada.
struct Saved {
    path: PathBuf,
    png: Vec<u8>,
    width: u32,
    height: u32,
    from_clipboard: bool,
}

impl Workspace {
    /// Engancha los pedidos de imágenes de una terminal nueva. La primera vez, además, borra las
    /// capturas viejas y (prueba de humo) entrega una imagen sintética.
    pub(super) fn watch_paste(&self, view: &Entity<TerminalView>, window: &mut Window, cx: &mut Context<Self>) {
        static FIRST: std::sync::Once = std::sync::Once::new();
        let mut first = false;
        FIRST.call_once(|| first = true);
        if first {
            std::thread::spawn(|| {
                ip::cleanup(&ip::captures_dir());
            });
            self.smoke_paste_image(view, cx);
        }
        cx.subscribe_in(view, window, |this, view, ev: &PasteEvent, window, cx| {
            this.on_paste_event(view.clone(), ev.clone(), window, cx)
        })
        .detach();
    }

    /// Cada lectura del monitor: qué agente corre en cada terminal (la barra solo se redibuja si
    /// cambió). En las remotas Kuntur no ve los agentes: sus imágenes quedan hasta que se quitan.
    pub(super) fn sync_image_bars(&self, cx: &mut Context<Self>) {
        for e in self.sessions.iter() {
            let keep = e.value.remote.is_some();
            for (docked, view) in [(false, Some(&e.value.view)), (true, e.value.dock.as_ref())] {
                let Some(view) = view else {
                    continue;
                };
                let agent = self
                    .running
                    .iter()
                    .find(|(s, d, _)| *s == e.id && *d == docked)
                    .map(|(_, _, a)| SharedString::from(self.agent_name(&a.agent)));
                view.update(cx, |v, cx| v.set_image_agent(agent, keep, cx));
            }
        }
    }

    fn paste_target(&self, view: &Entity<TerminalView>) -> Option<PasteTarget> {
        let (entry, docked) = self.sessions.iter().find_map(|e| {
            if &e.value.view == view {
                Some((e, false))
            } else if e.value.dock.as_ref() == Some(view) {
                Some((e, true))
            } else {
                None
            }
        })?;
        let agent = self
            .running
            .iter()
            .find(|(s, d, _)| *s == entry.id && *d == docked)
            .and_then(|(_, _, a)| self.catalog.get(&a.agent))
            .map(|def| (def.name.clone(), def.image_paste));
        Some(PasteTarget {
            agent,
            remote: entry.value.remote.as_ref().map(|r| r.session.clone()),
            remote_dir: agent_dir(&entry.name),
            shell: ShellFlavor::from_program(entry.value.shell.as_deref()),
        })
    }

    fn on_paste_event(
        &mut self,
        view: Entity<TerminalView>,
        ev: PasteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            PasteEvent::NonText => {
                let Some(item) = cx.read_from_clipboard() else {
                    return;
                };
                let mut files = Vec::new();
                for entry in item.entries() {
                    match entry {
                        ClipboardEntry::Image(image) => {
                            self.deliver_images(&view, vec![Source::Clipboard(image.bytes.clone())], cx);
                            return;
                        }
                        ClipboardEntry::ExternalPaths(paths) => files.extend(paths.paths().iter().cloned()),
                        ClipboardEntry::String(_) => {}
                    }
                }
                self.paste_files(&view, files, cx);
            }
            PasteEvent::Dropped(paths) => self.paste_files(&view, paths, cx),
            PasteEvent::OpenImage(path) => self.open_preview(path, None, window, cx),
            PasteEvent::PickImage => self.pick_image(view, cx),
        }
    }

    /// Archivos pegados o soltados: las imágenes se entregan como imagen; el resto, como ruta (en
    /// una pestaña remota se suben primero).
    fn paste_files(&mut self, view: &Entity<TerminalView>, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let (images, others): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| ip::is_image_path(p));
        if !images.is_empty() {
            self.deliver_images(view, images.into_iter().map(Source::File).collect(), cx);
        }
        let others: Vec<PathBuf> = others.into_iter().filter(|p| p.is_file()).collect();
        if others.is_empty() {
            return;
        }
        let Some(target) = self.paste_target(view) else {
            return;
        };
        match target.remote.clone() {
            Some(session) => {
                let view = view.downgrade();
                cx.spawn(async move |_, cx| {
                    let files: Vec<(String, Vec<u8>)> = others
                        .iter()
                        .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), std::fs::read(p).ok()?)))
                        .collect();
                    let dir = target.remote_dir.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move { upload(&session, &dir, &files) })
                        .await;
                    let _ = view.update(cx, |v, cx| match result {
                        Ok(remote) => {
                            v.paste_text(ip::paths_text(&remote, target.text_target()), Duration::ZERO);
                            v.image_notice(t(T::FilesPathsPasted), cx);
                        }
                        Err(e) => v.image_notice(format!("{}: {e}", t(T::ImageFailed)), cx),
                    });
                })
                .detach();
            }
            None => {
                let texts: Vec<String> = others.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                view.read(cx)
                    .paste_text(ip::paths_text(&texts, target.text_target()), Duration::ZERO);
            }
        }
    }

    /// "Imagen…": elegir archivos con el diálogo del sistema.
    fn pick_image(&mut self, view: Entity<TerminalView>, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(t(T::ImagePickPrompt).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |w, cx| w.paste_files(&view, paths, cx));
            }
        })
        .detach();
    }

    /// Prepara (PNG, ≤ 2000 px), guarda una copia local y entrega cada imagen según el agente.
    fn deliver_images(&mut self, view: &Entity<TerminalView>, sources: Vec<Source>, cx: &mut Context<Self>) {
        let Some(target) = self.paste_target(view) else {
            return;
        };
        let delivery = ip::choose(target.agent.as_ref().map(|a| a.1), target.remote.is_some());
        if delivery == Delivery::RemotePath {
            view.update(cx, |v, cx| v.image_notice(t(T::ImageUploading), cx));
        }
        let view = view.downgrade();
        cx.spawn(async move |_, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { prepare_all(sources) })
                .await;
            let (saved, errors) = prepared;
            if saved.is_empty() {
                let msg = errors.first().cloned().unwrap_or_default();
                let _ = view.update(cx, |v, cx| v.image_notice(format!("{}: {msg}", t(T::ImageFailed)), cx));
                return;
            }
            // Miniaturas en la barra.
            let numbers: Vec<usize> = view
                .update(cx, |v, cx| {
                    saved
                        .iter()
                        .map(|s| v.push_image(s.path.clone(), s.width, s.height, cx))
                        .collect()
                })
                .unwrap_or_default();
            match delivery {
                Delivery::Key(key) => {
                    let agent = target.agent.as_ref().map(|a| a.0.clone()).unwrap_or_default();
                    for (i, s) in saved.iter().enumerate() {
                        if i > 0 {
                            cx.background_executor().timer(KEY_GAP).await;
                        }
                        // Una imagen que vino de un archivo pasa antes por el portapapeles.
                        if !s.from_clipboard {
                            let png = s.png.clone();
                            if !put_on_clipboard(png, cx).await {
                                continue;
                            }
                        }
                        let n = numbers.get(i).copied().unwrap_or(0);
                        let _ = view.update(cx, |v, cx| {
                            v.write_bytes(&key);
                            v.image_notice(format!("{} {agent} (#{n})", t(T::ImageSentTo)), cx);
                        });
                    }
                }
                Delivery::LocalPath => {
                    let paths: Vec<String> = saved.iter().map(|s| s.path.to_string_lossy().into_owned()).collect();
                    let text = ip::paths_text(&paths, target.text_target());
                    let _ = view.update(cx, |v, cx| {
                        v.paste_text(text, Duration::ZERO);
                        v.image_notice(t(T::ImagePathPasted), cx);
                    });
                }
                Delivery::RemotePath => {
                    let Some(session) = target.remote.clone() else {
                        return;
                    };
                    let files: Vec<(String, Vec<u8>)> = saved
                        .iter()
                        .filter_map(|s| Some((s.path.file_name()?.to_string_lossy().into_owned(), s.png.clone())))
                        .collect();
                    let dir = target.remote_dir.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move { upload(&session, &dir, &files) })
                        .await;
                    let _ = view.update(cx, |v, cx| match result {
                        Ok(remote) => {
                            v.paste_text(ip::paths_text(&remote, target.text_target()), Duration::ZERO);
                            v.image_notice(t(T::ImageUploaded), cx);
                        }
                        Err(e) => v.image_notice(format!("{}: {e}", t(T::ImageFailed)), cx),
                    });
                }
            }
        })
        .detach();
    }

    /// Prueba de humo: `KUNTUR_SMOKE_PASTE_IMAGE=<png sintético>` entrega esa imagen en la primera
    /// terminal a los 5 s, como si se hubiera elegido con "Imagen…". Nunca toca el portapapeles real
    /// si no hay un agente con tecla propia (en la prueba no lo hay: se pega la ruta).
    fn smoke_paste_image(&self, view: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let Some(path) = std::env::var_os("KUNTUR_SMOKE_PASTE_IMAGE").map(PathBuf::from) else {
            return;
        };
        let view = view.downgrade();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |w, cx| {
                if let Some(view) = view.upgrade() {
                    w.deliver_images(&view, vec![Source::File(path)], cx);
                }
            });
        })
        .detach();
    }
}

/// Fuera del hilo de la UI: leer, preparar y guardar cada imagen. Devuelve las guardadas y los errores.
fn prepare_all(sources: Vec<Source>) -> (Vec<Saved>, Vec<String>) {
    let dir = ip::captures_dir();
    let mut saved = Vec::new();
    let mut errors = Vec::new();
    for source in sources {
        let (bytes, from_clipboard) = match source {
            Source::Clipboard(bytes) => (bytes, true),
            Source::File(path) => match std::fs::read(&path) {
                Ok(bytes) => (bytes, false),
                Err(e) => {
                    errors.push(e.to_string());
                    continue;
                }
            },
        };
        let prepared = match ip::prepare_png(&bytes) {
            Ok(p) => p,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        match ip::save(&dir, &prepared.png, &ip::capture_name_now()) {
            Ok(path) => saved.push(Saved {
                path,
                png: prepared.png,
                width: prepared.width,
                height: prepared.height,
                from_clipboard,
            }),
            Err(e) => errors.push(e.to_string()),
        }
    }
    (saved, errors)
}

/// Sube los archivos a `dir` del servidor (SFTP, F7) y devuelve sus rutas remotas.
fn upload(session: &RemoteSession, dir: &str, files: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
    let fs = session.sftp().map_err(|e| e.to_string())?;
    fs.create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (name, data) in files {
        let path = PathStyle::Posix.join(dir, name);
        fs.write(&path, data).map_err(|e| e.to_string())?;
        out.push(path);
    }
    Ok(out)
}

/// Pone un PNG en el portapapeles para que el agente lo lea con su tecla. En Windows con `CF_DIB`
/// (lo que lee Claude Code); en Linux/Mac con el portapapeles de GPUI.
async fn put_on_clipboard(png: Vec<u8>, cx: &mut AsyncApp) -> bool {
    #[cfg(windows)]
    {
        let _ = &cx;
        cx.background_executor()
            .spawn(async move { crate::clipboard_image::write_png(&png).is_ok() })
            .await
    }
    #[cfg(not(windows))]
    {
        let image = gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, png);
        cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_image(&image)))
            .is_ok()
    }
}

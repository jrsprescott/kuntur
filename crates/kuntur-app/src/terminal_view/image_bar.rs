//! Barra del agente estilo Warp (ajustes v0.2): franja fina sobre la terminal, solo mientras corre
//! un agente en ella (o quedan imágenes / un aviso), con las miniaturas de las imágenes pegadas
//! (#1, #2…), el botón "Imagen…" y un aviso corto. Clic en una miniatura = vista previa (F8).
//!
//! La terminal no sabe de agentes ni de SSH: el `Workspace` le dice qué agente corre, agrega las
//! imágenes que entregó y recibe los pedidos ([`PasteEvent`]).

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::{h_flex, tooltip::Tooltip};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::i18n::{T, t};

/// Alto de la barra.
pub const BAR_HEIGHT: f32 = 30.0;
const THUMB_H: f32 = 20.0;
/// Cuánto se ve un aviso.
const NOTICE_FOR: Duration = Duration::from_secs(5);

/// Pedidos de la terminal al `Workspace` sobre imágenes y archivos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteEvent {
    /// Ctrl+V (o clic derecho) sin texto en el portapapeles: el `Workspace` mira si hay una imagen
    /// o archivos copiados.
    NonText,
    /// Archivos soltados sobre la terminal (desde el Explorador).
    Dropped(Vec<PathBuf>),
    /// Clic en una miniatura: abrirla en la vista previa.
    OpenImage(PathBuf),
    /// Botón "Imagen…": elegir un archivo.
    PickImage,
}

/// Una imagen entregada en esta terminal.
#[derive(Clone, Debug, PartialEq)]
pub struct PastedImage {
    pub n: usize,
    /// Copia local (PNG) para la miniatura y la vista previa.
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
pub struct ImageBar {
    /// Agente que corre en esta terminal (nombre para mostrar).
    agent: Option<SharedString>,
    images: Vec<PastedImage>,
    next: usize,
    notice: Option<SharedString>,
    notice_task: Option<Task<()>>,
}

impl ImageBar {
    pub fn visible(&self) -> bool {
        self.agent.is_some() || !self.images.is_empty() || self.notice.is_some()
    }

    /// Cambió el agente de la terminal. Si dejó de correr, sus imágenes se van con él (`keep` =
    /// conservarlas, por ejemplo en una pestaña remota, donde Kuntur no ve los agentes). `true` si
    /// algo cambió.
    pub fn set_agent(&mut self, agent: Option<SharedString>, keep: bool) -> bool {
        if agent == self.agent {
            return false;
        }
        if agent.is_none() && !keep {
            self.images.clear();
            self.next = 0;
        }
        self.agent = agent;
        true
    }

    /// Agrega una imagen; devuelve su número (#n).
    pub fn push(&mut self, path: PathBuf, width: u32, height: u32) -> usize {
        self.next += 1;
        self.images.push(PastedImage {
            n: self.next,
            path,
            width,
            height,
        });
        self.next
    }

    fn clear(&mut self) {
        self.images.clear();
        self.next = 0;
    }
}

/// Ancho de la miniatura para un alto fijo, sin pasar de 3:1.
fn thumb_width(width: u32, height: u32) -> f32 {
    if height == 0 {
        return THUMB_H;
    }
    (THUMB_H * width as f32 / height as f32).clamp(THUMB_H * 0.5, THUMB_H * 3.0)
}

impl super::TerminalView {
    /// El `Workspace` informa qué agente corre en esta terminal (en cada lectura del monitor). Solo
    /// redibuja si cambió (nada de redibujos en reposo).
    pub fn set_image_agent(&mut self, agent: Option<SharedString>, keep: bool, cx: &mut Context<Self>) {
        if self.image_bar.set_agent(agent, keep) {
            cx.notify();
        }
    }

    /// Imagen entregada: aparece en la barra. Devuelve su número.
    pub fn push_image(&mut self, path: PathBuf, width: u32, height: u32, cx: &mut Context<Self>) -> usize {
        let n = self.image_bar.push(path, width, height);
        cx.notify();
        n
    }

    /// Aviso corto en la barra (se va solo a los pocos segundos).
    pub fn image_notice(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.image_bar.notice = Some(text.into());
        self.image_bar.notice_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_FOR).await;
            let _ = this.update(cx, |v, cx| {
                v.image_bar.notice = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Lee el portapapeles: si hay texto lo pega; si solo hay una imagen o archivos, se lo pide al
    /// `Workspace` (nunca pega la ruta "pegada" que GPUI arma con los archivos).
    pub(super) fn paste_or_request(&self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let text: String = item
            .entries()
            .iter()
            .filter_map(|e| match e {
                ClipboardEntry::String(s) => Some(s.text().as_str()),
                _ => None,
            })
            .collect();
        if !text.is_empty() {
            let _ = self.pane.paste(&text);
        } else if item
            .entries()
            .iter()
            .any(|e| matches!(e, ClipboardEntry::Image(_) | ClipboardEntry::ExternalPaths(_)))
        {
            cx.emit(PasteEvent::NonText);
        }
    }

    /// La franja (si corresponde). Colores sacados de la paleta de la terminal.
    pub(super) fn render_image_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bar = &self.image_bar;
        if !bar.visible() {
            return None;
        }
        let fg: Hsla = super::to_color(self.palette.fg);
        let accent: Hsla = super::to_color(self.palette.cursor);
        let muted = fg.opacity(0.62);
        let hover = fg.opacity(0.09);
        let thumbs = bar.images.iter().map(|im| {
            let path = im.path.clone();
            let tip: SharedString =
                format!("#{} · {}×{} · {}", im.n, im.width, im.height, t(T::ImageOpenPreview)).into();
            h_flex()
                .id(("pasted-image", im.n))
                .flex_shrink_0()
                .gap(px(4.))
                .px(px(3.))
                .h(px(THUMB_H + 4.))
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(PasteEvent::OpenImage(path.clone()))))
                .child(
                    img(im.path.clone())
                        .h(px(THUMB_H))
                        .w(px(thumb_width(im.width, im.height)))
                        .rounded(px(3.))
                        .object_fit(ObjectFit::Cover),
                )
                .child(div().text_size(px(11.)).text_color(muted).child(format!("#{}", im.n)))
        });
        let button = |id: &'static str, label: SharedString, tip: &'static str| {
            div()
                .id(id)
                .flex_shrink_0()
                .px(px(7.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .text_size(px(11.5))
                .text_color(muted)
                .cursor_pointer()
                .hover(move |s| s.bg(hover).text_color(fg))
                .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                .child(label)
        };
        let has_images = !bar.images.is_empty();
        Some(
            h_flex()
                .id("image-bar")
                .flex_shrink_0()
                .w_full()
                .h(px(BAR_HEIGHT))
                .mb(px(4.))
                .px(px(6.))
                .gap(px(6.))
                .items_center()
                .rounded(px(7.))
                .bg(fg.opacity(0.05))
                .border_1()
                .border_color(fg.opacity(0.10))
                // Los clics en la barra no llegan a la terminal (selección, mouse del programa).
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .when_some(bar.agent.clone(), |d, agent| {
                    d.child(
                        h_flex()
                            .flex_shrink_0()
                            .gap(px(5.))
                            .items_center()
                            .child(div().size(px(6.)).rounded_full().bg(accent))
                            .child(div().text_size(px(11.5)).text_color(fg).child(agent)),
                    )
                })
                .child(
                    h_flex()
                        .id("pasted-images")
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .items_center()
                        .overflow_x_scroll()
                        .children(thumbs),
                )
                .when_some(bar.notice.clone(), |d, notice| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .max_w(px(260.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(notice),
                    )
                })
                .child(
                    button("image-pick", t(T::ImagePick).into(), t(T::ImagePickTip))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(PasteEvent::PickImage))),
                )
                .when(has_images, |d| {
                    d.child(
                        button("image-clear", "×".into(), t(T::ImageClearTip)).on_click(cx.listener(
                            |this: &mut super::TerminalView, _, _, cx| {
                                this.image_bar.clear();
                                cx.notify();
                            },
                        )),
                    )
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    // Importes explícitos: `super::*` trae la macro `test` de GPUI y rompe `#[test]`.
    use super::{ImageBar, THUMB_H, thumb_width};

    #[test]
    fn la_barra_se_ve_con_agente_imagenes_o_aviso() {
        let mut bar = ImageBar::default();
        assert!(!bar.visible());
        assert!(bar.set_agent(Some("Claude Code".into()), false));
        assert!(
            !bar.set_agent(Some("Claude Code".into()), false),
            "sin cambios no redibuja"
        );
        assert!(bar.visible());
        assert_eq!(bar.push("a.png".into(), 10, 10), 1);
        assert_eq!(bar.push("b.png".into(), 10, 10), 2);
        // El agente terminó: sus imágenes se van y la numeración vuelve a empezar.
        assert!(bar.set_agent(None, false));
        assert!(!bar.visible());
        assert_eq!(bar.push("c.png".into(), 10, 10), 1);
    }

    #[test]
    fn en_remoto_las_imagenes_quedan() {
        let mut bar = ImageBar::default();
        bar.push("a.png".into(), 10, 10);
        bar.set_agent(None, true);
        assert!(bar.visible());
    }

    #[test]
    fn miniatura_proporcional_con_limites() {
        assert_eq!(thumb_width(100, 100), THUMB_H);
        assert_eq!(thumb_width(1920, 1080), THUMB_H * 1920.0 / 1080.0);
        assert_eq!(thumb_width(5000, 100), THUMB_H * 3.0);
        assert_eq!(thumb_width(10, 1000), THUMB_H * 0.5);
        assert_eq!(thumb_width(10, 0), THUMB_H);
    }
}

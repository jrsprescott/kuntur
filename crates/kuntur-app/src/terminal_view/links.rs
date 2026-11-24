//! Enlaces en la salida (F8, especificación §9): con **Ctrl** pulsado se subraya la ruta o URL
//! bajo el mouse; **Ctrl+clic** la abre; **clic derecho** sobre ella pide el menú contextual.
//! La detección es pura (`kuntur_term::links`); aquí solo se conecta con el mouse y se avisa al
//! `Workspace` con [`LinkEvent`].

use kuntur_term::{Link, LinkSpan, link_at, links::resolve, links_in_row};

use super::*;

/// Avisos de enlaces a quien contiene la terminal.
#[derive(Clone, Debug)]
pub enum LinkEvent {
    /// Ctrl+clic: abrir (URL → navegador; ruta → vista previa).
    Open(Link),
    /// Clic derecho sobre un enlace: menú contextual en esa posición de la ventana.
    Menu(Link, Point<Pixels>),
}

impl EventEmitter<LinkEvent> for TerminalView {}

/// Enlace subrayado: fila, columnas `[start, end)` y destino.
#[derive(Clone, Debug)]
pub(super) struct HoverLink {
    pub row: usize,
    pub start: usize,
    pub end: usize,
    pub link: Link,
}

impl TerminalView {
    /// Enlace (ya comprobado: la ruta existe) en la celda bajo `pos`.
    fn link_under(&self, pos: Point<Pixels>, window: &mut Window) -> Option<HoverLink> {
        let m = self.metrics(window);
        let (col, row, _) = self.cell_at(pos, m);
        let screen = self.pane.with_emulator(|e| Screen::capture(e, &self.palette));
        let span = link_at(&screen, row, col)?;
        self.resolve_span(row, &span)
    }

    fn resolve_span(&self, row: usize, span: &LinkSpan) -> Option<HoverLink> {
        let cwd = self.pane.cwd();
        let link = resolve(span, cwd.as_deref(), |p| p.exists())?;
        Some(HoverLink {
            row,
            start: span.start,
            end: span.end,
            link,
        })
    }

    /// Movimiento del mouse: con Ctrl se busca el enlace bajo el puntero; sin Ctrl se quita.
    pub(super) fn update_hover(
        &mut self,
        pos: Point<Pixels>,
        mods: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_mouse = Some(pos);
        let hover = if mods.control && !mods.alt && !self.selecting {
            self.link_under(pos, window)
        } else {
            None
        };
        let changed = match (&hover, &self.hover_link) {
            (Some(a), Some(b)) => (a.row, a.start, a.end) != (b.row, b.start, b.end),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.hover_link = hover;
            cx.notify();
        }
    }

    /// Ctrl pulsado o soltado sin mover el mouse.
    pub(super) fn on_modifiers(&mut self, ev: &ModifiersChangedEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.last_mouse {
            self.update_hover(pos, ev.modifiers, window, cx);
        }
    }

    /// Clic sobre un enlace. Devuelve `true` si lo atendió (y el clic no sigue a la selección ni al
    /// programa): Ctrl+clic izquierdo lo abre; clic derecho sin selección pide el menú.
    pub(super) fn link_click(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = ev.modifiers;
        match ev.button {
            MouseButton::Left if m.control && !m.alt && !m.shift => match self.link_under(ev.position, window) {
                Some(h) => {
                    self.hover_link = None;
                    cx.emit(LinkEvent::Open(h.link));
                    cx.notify();
                    true
                }
                None => false,
            },
            MouseButton::Right if !m.shift && self.mouse_reporting(false).is_none() => {
                if self.pane.with_emulator(|e| e.has_selection()) {
                    return false;
                }
                match self.link_under(ev.position, window) {
                    Some(h) => {
                        cx.emit(LinkEvent::Menu(h.link, ev.position));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }

    /// Prueba de humo: subraya todos los enlaces visibles y pide el menú del primero que sea una
    /// ruta (para capturar la detección sin mover el mouse real).
    pub fn smoke_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_all_links = true;
        let m = self.metrics(window);
        let screen = self.pane.with_emulator(|e| Screen::capture(e, &self.palette));
        let b = self.bounds.get();
        for row in 0..screen.rows {
            for span in links_in_row(&screen, row) {
                if let Some(h) = self.resolve_span(row, &span)
                    && matches!(h.link, Link::File { .. })
                {
                    let pos = point(
                        b.origin.x + m.cell_w * (span.start as f32 + 1.0),
                        b.origin.y + m.line_h * (row as f32 + 1.0),
                    );
                    cx.emit(LinkEvent::Menu(h.link, pos));
                    cx.notify();
                    return;
                }
            }
        }
        cx.notify();
    }

    /// Enlaces a subrayar en este cuadro: el que está bajo el mouse con Ctrl, o todos (humo).
    pub(super) fn underlined_links(&self, screen: &Screen) -> Vec<(usize, usize, usize)> {
        if self.show_all_links {
            return (0..screen.rows)
                .flat_map(|row| {
                    links_in_row(screen, row)
                        .into_iter()
                        .filter(move |s| self.resolve_span(row, s).is_some())
                        .map(move |s| (row, s.start, s.end))
                })
                .collect();
        }
        self.hover_link.iter().map(|h| (h.row, h.start, h.end)).collect()
    }
}

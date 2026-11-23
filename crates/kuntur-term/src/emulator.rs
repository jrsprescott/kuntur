use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Config, Osc52, Term, TermMode, viewport_to_point};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use crate::mouse::{MouseMode, MouseReporting};
use crate::size::GridSize;

/// Recibe los eventos que emite el emulador (respuestas a la terminal, portapapeles, título…).
#[derive(Clone, Default)]
pub struct EventSink(Arc<Mutex<Vec<Event>>>);

impl EventListener for EventSink {
    fn send_event(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl EventSink {
    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Tipo de selección con el ratón.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    /// Arrastrar.
    Simple,
    /// Doble clic: palabra.
    Word,
    /// Triple clic: línea.
    Line,
    /// Alt + arrastrar: bloque rectangular.
    Block,
}

/// Emulador VT: estado de la grilla + parser de secuencias de escape.
pub struct Emulator {
    term: Term<EventSink>,
    parser: Processor<StdSyncHandler>,
    events: EventSink,
}

impl Emulator {
    pub fn new(size: GridSize) -> Self {
        let events = EventSink::default();
        let config = Config {
            osc52: Osc52::OnlyCopy,
            ..Config::default()
        };
        let term = Term::new(config, &size, events.clone());
        Self {
            term,
            parser: Processor::new(),
            events,
        }
    }

    /// Procesa bytes que llegan del programa (salida del PTY).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, size: GridSize) {
        self.term.resize(size);
    }

    pub fn take_events(&self) -> Vec<Event> {
        self.events.drain()
    }

    pub fn term(&self) -> &Term<EventSink> {
        &self.term
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Modo de mouse que pidió el programa (nvim, htop, opencode…).
    pub fn mouse_mode(&self) -> MouseMode {
        let m = self.mode();
        let report = if m.contains(TermMode::MOUSE_MOTION) {
            MouseReporting::Motion
        } else if m.contains(TermMode::MOUSE_DRAG) {
            MouseReporting::Drag
        } else if m.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseReporting::Click
        } else {
            MouseReporting::Off
        };
        MouseMode {
            report,
            sgr: m.contains(TermMode::SGR_MOUSE),
        }
    }

    /// Texto de una fila visible (sin scroll), sin espacios al final.
    pub fn line_text(&self, row: usize) -> String {
        let grid = self.term.grid();
        let cols = self.term.grid().columns();
        let line = &grid[Line(row as i32)];
        let s: String = (0..cols).map(|c| line[Column(c)].c).collect();
        s.trim_end().to_string()
    }

    /// ¿Alguna fila visible contiene este texto? (útil en pruebas)
    pub fn contains_text(&self, needle: &str) -> bool {
        (0..self.term.screen_lines()).any(|r| self.line_text(r).contains(needle))
    }

    /// Desplaza el historial (positivo = hacia arriba).
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn grid_point(&self, col: usize, row: usize) -> Point {
        viewport_to_point(self.display_offset(), Point::new(row, Column(col)))
    }

    pub fn start_selection(&mut self, kind: SelectionKind, col: usize, row: usize, side: Side) {
        let ty = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        };
        let p = self.grid_point(col, row);
        self.term.selection = Some(Selection::new(ty, p, side));
    }

    pub fn update_selection(&mut self, col: usize, row: usize, side: Side) {
        let p = self.grid_point(col, row);
        if let Some(sel) = self.term.selection.as_mut() {
            sel.update(p, side);
        }
    }

    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    /// Texto seleccionado, sin códigos ANSI y uniendo las líneas cortadas solo por el ancho.
    /// Sin salto de línea final: así, al pegar un comando copiado, no se ejecuta solo.
    pub fn selection_text(&self) -> Option<String> {
        let text = self.term.selection_to_string()?;
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        (!text.is_empty()).then_some(text)
    }

    pub fn has_selection(&self) -> bool {
        self.term.selection.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribe_texto_plano() {
        let mut emu = Emulator::new(GridSize::new(20, 4));
        emu.feed(b"hola\r\nmundo");
        assert_eq!(emu.line_text(0), "hola");
        assert_eq!(emu.line_text(1), "mundo");
    }

    #[test]
    fn respeta_colores_ansi() {
        let mut emu = Emulator::new(GridSize::new(20, 2));
        emu.feed(b"\x1b[31mR\x1b[0mN");
        let grid = emu.term().grid();
        use alacritty_terminal::vte::ansi::{Color, NamedColor};
        assert_eq!(grid[Line(0)][Column(0)].fg, Color::Named(NamedColor::Red));
        assert_eq!(grid[Line(0)][Column(1)].fg, Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn responde_consulta_de_posicion_del_cursor() {
        // ConPTY manda ESC[6n al arrancar y espera esta respuesta.
        let mut emu = Emulator::new(GridSize::new(20, 4));
        emu.feed(b"ab\x1b[6n");
        let respuestas: Vec<String> = emu
            .take_events()
            .into_iter()
            .filter_map(|e| if let Event::PtyWrite(s) = e { Some(s) } else { None })
            .collect();
        assert_eq!(respuestas, vec!["\x1b[1;3R".to_string()]);
    }

    #[test]
    fn selecciona_y_copia_texto() {
        let mut emu = Emulator::new(GridSize::new(20, 3));
        emu.feed(b"git status\r\nlimpio");
        emu.start_selection(SelectionKind::Simple, 0, 0, Side::Left);
        emu.update_selection(2, 0, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("git"));
    }

    #[test]
    fn une_lineas_cortadas_por_el_ancho() {
        let mut emu = Emulator::new(GridSize::new(5, 3));
        emu.feed(b"abcdefgh");
        emu.start_selection(SelectionKind::Line, 0, 0, Side::Left);
        emu.update_selection(4, 1, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("abcdefgh"));
    }

    #[test]
    fn modo_bracketed_paste() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        assert!(!emu.mode().contains(TermMode::BRACKETED_PASTE));
        emu.feed(b"\x1b[?2004h");
        assert!(emu.mode().contains(TermMode::BRACKETED_PASTE));
    }

    #[test]
    fn osc52_entrega_texto_al_portapapeles() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        // "hola" en base64 = aG9sYQ==
        emu.feed(b"\x1b]52;c;aG9sYQ==\x07");
        let copiado: Vec<String> = emu
            .take_events()
            .into_iter()
            .filter_map(|e| {
                if let Event::ClipboardStore(_, s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(copiado, vec!["hola".to_string()]);
    }

    #[test]
    fn detecta_modo_de_mouse() {
        let mut emu = Emulator::new(GridSize::new(10, 2));
        assert_eq!(
            emu.mouse_mode(),
            MouseMode {
                report: MouseReporting::Off,
                sgr: false
            }
        );
        emu.feed(b"\x1b[?1002h\x1b[?1006h");
        assert_eq!(
            emu.mouse_mode(),
            MouseMode {
                report: MouseReporting::Drag,
                sgr: true
            }
        );
    }
}

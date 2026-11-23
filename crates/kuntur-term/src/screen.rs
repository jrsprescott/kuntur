use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;

use crate::emulator::Emulator;
use crate::palette::{Palette, Rgb8};

/// Una celda lista para dibujar (colores ya resueltos a RGB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellView {
    pub ch: char,
    pub fg: Rgb8,
    pub bg: Rgb8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Ocupa dos columnas (CJK, emoji).
    pub wide: bool,
    pub selected: bool,
}

/// Foto de la parte visible de la terminal. La UI solo lee esto: no toca el emulador.
#[derive(Clone, Debug)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<CellView>,
    /// (columna, fila) del cursor si está visible.
    pub cursor: Option<(usize, usize)>,
    pub default_bg: Rgb8,
}

impl Screen {
    pub fn capture(emu: &Emulator, pal: &Palette) -> Screen {
        let term = emu.term();
        let cols = term.columns();
        let rows = term.screen_lines();
        let blank = CellView {
            ch: ' ',
            fg: pal.fg,
            bg: pal.bg,
            bold: false,
            italic: false,
            underline: false,
            wide: false,
            selected: false,
        };
        let mut cells = vec![blank; cols * rows];

        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        let cursor = content.cursor;

        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset;
            let col = indexed.point.column.0;
            if row < 0 || row as usize >= rows || col >= cols {
                continue;
            }
            let cell = indexed.cell;
            let flags = cell.flags;
            let mut fg = pal.resolve(cell.fg);
            let mut bg = pal.resolve(cell.bg);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let selected = selection.is_some_and(|s| s.contains(indexed.point));
            if selected {
                bg = pal.selection_bg;
            }
            let ch = if flags.contains(Flags::WIDE_CHAR_SPACER) {
                ' '
            } else {
                cell.c
            };
            cells[row as usize * cols + col] = CellView {
                ch,
                fg,
                bg,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                wide: flags.contains(Flags::WIDE_CHAR),
                selected,
            };
        }

        let cursor_pos = (cursor.shape != CursorShape::Hidden)
            .then(|| (cursor.point.column.0, cursor.point.line.0 + offset))
            .filter(|&(c, r)| r >= 0 && (r as usize) < rows && c < cols)
            .map(|(c, r)| (c, r as usize));

        Screen {
            cols,
            rows,
            cells,
            cursor: cursor_pos,
            default_bg: pal.bg,
        }
    }

    pub fn cell(&self, col: usize, row: usize) -> &CellView {
        &self.cells[row * self.cols + col]
    }

    pub fn row_text(&self, row: usize) -> String {
        let s: String = (0..self.cols).map(|c| self.cell(c, row).ch).collect();
        s.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size::GridSize;

    #[test]
    fn captura_texto_colores_y_cursor() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 3));
        emu.feed(b"\x1b[31mhola\x1b[0m");
        let s = Screen::capture(&emu, &pal);
        assert_eq!(s.row_text(0), "hola");
        assert_eq!(s.cell(0, 0).fg, pal.ansi[1]);
        assert_eq!(s.cursor, Some((4, 0)));
    }

    #[test]
    fn marca_la_seleccion() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 2));
        emu.feed(b"abc");
        emu.start_selection(
            crate::SelectionKind::Simple,
            0,
            0,
            alacritty_terminal::index::Side::Left,
        );
        emu.update_selection(1, 0, alacritty_terminal::index::Side::Right);
        let s = Screen::capture(&emu, &pal);
        assert!(s.cell(0, 0).selected && s.cell(1, 0).selected);
        assert!(!s.cell(2, 0).selected);
        assert_eq!(s.cell(0, 0).bg, pal.selection_bg);
    }

    #[test]
    fn video_inverso() {
        let pal = Palette::dark();
        let mut emu = Emulator::new(GridSize::new(10, 2));
        emu.feed(b"\x1b[7mX");
        let s = Screen::capture(&emu, &pal);
        assert_eq!(s.cell(0, 0).fg, pal.bg);
        assert_eq!(s.cell(0, 0).bg, pal.fg);
    }
}

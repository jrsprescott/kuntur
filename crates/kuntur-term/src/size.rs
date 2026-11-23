use alacritty_terminal::grid::Dimensions;

/// Tamaño de la grilla en celdas (columnas × filas visibles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl GridSize {
    /// Crea un tamaño válido: mínimo 2 columnas y 1 fila.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.max(2),
            rows: rows.max(1),
        }
    }

    /// Calcula cuántas celdas caben en un área en píxeles.
    pub fn from_pixels(width: f32, height: f32, cell_w: f32, cell_h: f32) -> Self {
        let cols = (width / cell_w).floor().max(0.0) as u16;
        let rows = (height / cell_h).floor().max(0.0) as u16;
        Self::new(cols, rows)
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

impl From<GridSize> for portable_pty::PtySize {
    fn from(s: GridSize) -> Self {
        portable_pty::PtySize {
            rows: s.rows,
            cols: s.cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimo_valido() {
        assert_eq!(GridSize::new(0, 0), GridSize { cols: 2, rows: 1 });
    }

    #[test]
    fn desde_pixeles() {
        assert_eq!(
            GridSize::from_pixels(800.0, 600.0, 8.0, 20.0),
            GridSize { cols: 100, rows: 30 }
        );
    }
}

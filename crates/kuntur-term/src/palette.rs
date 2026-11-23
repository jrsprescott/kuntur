use alacritty_terminal::vte::ansi::{Color, NamedColor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn hex(v: u32) -> Self {
        Self {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }

    fn dim(self) -> Self {
        let f = |c: u8| (c as u16 * 2 / 3) as u8;
        Self {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
        }
    }
}

/// Colores de la terminal. Los 16 ANSI salen de `themes/*.toml` (la Fase 2 los leerá del archivo).
#[derive(Clone, Debug)]
pub struct Palette {
    pub ansi: [Rgb8; 16],
    pub fg: Rgb8,
    pub bg: Rgb8,
    pub cursor: Rgb8,
    pub selection_bg: Rgb8,
}

impl Palette {
    /// Tema Dark · Medianoche (themes/dark.toml).
    pub fn dark() -> Self {
        let h = Rgb8::hex;
        Self {
            ansi: [
                h(0x15161e),
                h(0xf7768e),
                h(0x9ece6a),
                h(0xe0af68),
                h(0x7aa2f7),
                h(0xbb9af7),
                h(0x7dcfff),
                h(0xa9b1d6),
                h(0x414868),
                h(0xff899d),
                h(0x9fe044),
                h(0xfaba4a),
                h(0x8db0ff),
                h(0xc7a9ff),
                h(0xa4daff),
                h(0xc0caf5),
            ],
            fg: h(0xd5dcea),
            bg: h(0x0b1019),
            cursor: h(0x7aa2f7),
            selection_bg: h(0x2b3a5c),
        }
    }

    /// Convierte un color del emulador (nombrado, índice 0-255 o RGB) a RGB.
    pub fn resolve(&self, color: Color) -> Rgb8 {
        match color {
            Color::Spec(c) => Rgb8 { r: c.r, g: c.g, b: c.b },
            Color::Indexed(i) => self.indexed(i),
            Color::Named(n) => self.named(n),
        }
    }

    fn indexed(&self, i: u8) -> Rgb8 {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb8 {
                    r: level(i / 36),
                    g: level((i / 6) % 6),
                    b: level(i % 6),
                }
            }
            232..=255 => {
                let g = 8 + (i - 232) * 10;
                Rgb8 { r: g, g, b: g }
            }
        }
    }

    fn named(&self, n: NamedColor) -> Rgb8 {
        let idx = n as usize;
        match n {
            NamedColor::Foreground | NamedColor::BrightForeground => self.fg,
            NamedColor::Background => self.bg,
            NamedColor::Cursor => self.cursor,
            NamedColor::DimForeground => self.fg.dim(),
            _ if idx < 16 => self.ansi[idx],
            // DimBlack..DimWhite: versión atenuada de los 8 básicos.
            _ => self.ansi[(idx - NamedColor::DimBlack as usize) % 8].dim(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn colores_nombrados_e_indices() {
        let p = Palette::dark();
        assert_eq!(p.resolve(Color::Named(NamedColor::Red)), p.ansi[1]);
        assert_eq!(p.resolve(Color::Named(NamedColor::Foreground)), p.fg);
        assert_eq!(p.resolve(Color::Indexed(9)), p.ansi[9]);
    }

    #[test]
    fn cubo_256_y_grises() {
        let p = Palette::dark();
        assert_eq!(p.resolve(Color::Indexed(196)), Rgb8 { r: 255, g: 0, b: 0 });
        assert_eq!(p.resolve(Color::Indexed(232)), Rgb8 { r: 8, g: 8, b: 8 });
    }

    #[test]
    fn truecolor_pasa_igual() {
        let p = Palette::dark();
        assert_eq!(
            p.resolve(Color::Spec(Rgb { r: 1, g: 2, b: 3 })),
            Rgb8 { r: 1, g: 2, b: 3 }
        );
    }
}

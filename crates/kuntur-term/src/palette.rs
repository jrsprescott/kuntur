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

    /// Luminancia relativa (WCAG 2), de 0 (negro) a 1 (blanco).
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// Relación de contraste WCAG entre dos colores (1 a 21).
    pub fn contrast(self, other: Rgb8) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Si este color de texto no se lee sobre `bg`, lo acerca a negro o a blanco (lo que más
    /// contraste dé con el fondo) hasta llegar a `min`. Así un tema claro sigue leyendo bien el
    /// "blanco" ANSI que usan PSReadLine y otros programas.
    pub fn with_min_contrast(self, bg: Rgb8, min: f32) -> Rgb8 {
        if self.contrast(bg) >= min {
            return self;
        }
        let white = Rgb8::hex(0xffffff);
        let black = Rgb8::hex(0x000000);
        let target = if white.contrast(bg) >= black.contrast(bg) {
            white
        } else {
            black
        };
        let mix = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        let mut out = self;
        for step in 1..=10 {
            let t = step as f32 / 10.0;
            out = Rgb8 {
                r: mix(self.r, target.r, t),
                g: mix(self.g, target.g, t),
                b: mix(self.b, target.b, t),
            };
            if out.contrast(bg) >= min {
                break;
            }
        }
        out
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
    pub fn new(ansi: [Rgb8; 16], fg: Rgb8, bg: Rgb8, cursor: Rgb8, selection_bg: Rgb8) -> Self {
        Self {
            ansi,
            fg,
            bg,
            cursor,
            selection_bg,
        }
    }

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

    #[test]
    fn contraste_minimo() {
        let blanco = Rgb8::hex(0xffffff);
        let negro = Rgb8::hex(0x000000);
        assert!((blanco.contrast(negro) - 21.0).abs() < 0.01);
        assert!((blanco.contrast(blanco) - 1.0).abs() < 0.01);
        // Gris claro sobre fondo casi blanco (tema White + PSReadLine): se oscurece hasta leerse.
        let fondo = Rgb8::hex(0xfbfcfe);
        let ajustado = Rgb8::hex(0xe5e7eb).with_min_contrast(fondo, 3.0);
        assert!(ajustado.contrast(fondo) >= 3.0);
        assert!(ajustado.r < 0xe5);
        // Azul apagado sobre azul oscuro (tema Blue): se aclara.
        let azul = Rgb8::hex(0x122771);
        let ajustado = Rgb8::hex(0x3b5bab).with_min_contrast(azul, 3.0);
        assert!(ajustado.contrast(azul) >= 3.0);
        assert!(ajustado.r > 0x3b);
        // Si ya se lee, no se toca.
        assert_eq!(
            Rgb8::hex(0xd5dcea).with_min_contrast(Rgb8::hex(0x0b1019), 3.0),
            Rgb8::hex(0xd5dcea)
        );
    }

    #[test]
    fn construye_desde_colores_de_tema() {
        let ansi = [Rgb8::hex(0x000000); 16];
        let p = Palette::new(
            ansi,
            Rgb8::hex(0xffffff),
            Rgb8::hex(0x101010),
            Rgb8::hex(0x00ff00),
            Rgb8::hex(0x333333),
        );
        assert_eq!(p.fg, Rgb8::hex(0xffffff));
        assert_eq!(p.resolve(Color::Named(NamedColor::Background)), Rgb8::hex(0x101010));
        assert_eq!(p.selection_bg, Rgb8::hex(0x333333));
    }
}

/// Teclas que la UI traduce a bytes para el programa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };

    /// Parámetro xterm de modificadores: 1 + shift + 2·alt + 4·ctrl.
    fn xterm_param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }

    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }
}

/// Convierte una tecla en los bytes que espera una terminal tipo xterm.
/// `app_cursor` = modo DECCKM activo (lo activan nvim, less, etc.).
pub fn encode_key(key: &Key, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    let csi = |final_byte: char| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[1;{}{}", mods.xterm_param(), final_byte).into_bytes()
        } else if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[{n};{}~", mods.xterm_param()).into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };

    let bytes = match key {
        Key::Char(c) => {
            let mut out = Vec::new();
            if mods.alt {
                out.push(0x1b);
            }
            if mods.ctrl {
                let b = match c.to_ascii_lowercase() {
                    l @ 'a'..='z' => l as u8 - b'a' + 1,
                    ' ' | '2' | '@' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '6' | '^' => 0x1e,
                    '-' | '_' | '7' => 0x1f,
                    _ => return None,
                };
                out.push(b);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out
        }
        Key::Enter => {
            if mods.alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        Key::Backspace => {
            if mods.ctrl {
                vec![0x08]
            } else if mods.alt {
                vec![0x1b, 0x7f]
            } else {
                vec![0x7f]
            }
        }
        Key::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => csi('A'),
        Key::Down => csi('B'),
        Key::Right => csi('C'),
        Key::Left => csi('D'),
        Key::Home => csi('H'),
        Key::End => csi('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F(n @ 1..=4) => {
            let f = b"PQRS"[(*n - 1) as usize] as char;
            if mods.any() {
                format!("\x1b[1;{}{}", mods.xterm_param(), f).into_bytes()
            } else {
                format!("\x1bO{f}").into_bytes()
            }
        }
        Key::F(n @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][(*n - 5) as usize];
            tilde(code)
        }
        Key::F(_) => return None,
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
    };
    const ALT: Mods = Mods {
        ctrl: false,
        alt: true,
        shift: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: true,
    };

    #[test]
    fn letras_y_utf8() {
        assert_eq!(encode_key(&Key::Char('a'), Mods::NONE, false), Some(b"a".to_vec()));
        assert_eq!(
            encode_key(&Key::Char('ñ'), Mods::NONE, false),
            Some("ñ".as_bytes().to_vec())
        );
    }

    #[test]
    fn ctrl_c_es_interrupcion() {
        assert_eq!(encode_key(&Key::Char('c'), CTRL, false), Some(vec![0x03]));
        assert_eq!(encode_key(&Key::Char('C'), CTRL, false), Some(vec![0x03]));
    }

    #[test]
    fn alt_antepone_escape() {
        assert_eq!(encode_key(&Key::Char('b'), ALT, false), Some(vec![0x1b, b'b']));
    }

    #[test]
    fn flechas_normales_y_modo_aplicacion() {
        assert_eq!(encode_key(&Key::Up, Mods::NONE, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(&Key::Up, Mods::NONE, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(encode_key(&Key::Right, CTRL, false), Some(b"\x1b[1;5C".to_vec()));
    }

    #[test]
    fn teclas_especiales() {
        assert_eq!(encode_key(&Key::Enter, Mods::NONE, false), Some(b"\r".to_vec()));
        assert_eq!(encode_key(&Key::Backspace, Mods::NONE, false), Some(vec![0x7f]));
        assert_eq!(encode_key(&Key::Tab, SHIFT, false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(encode_key(&Key::Delete, Mods::NONE, false), Some(b"\x1b[3~".to_vec()));
        assert_eq!(encode_key(&Key::F(1), Mods::NONE, false), Some(b"\x1bOP".to_vec()));
        assert_eq!(encode_key(&Key::F(5), Mods::NONE, false), Some(b"\x1b[15~".to_vec()));
    }
}

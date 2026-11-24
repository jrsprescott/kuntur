//! Diálogos de conexión **dentro de la terminal** (como hace `ssh`): mientras Kuntur pregunta algo
//! (huella nueva, passphrase, contraseña), las teclas no van al servidor sino a este editor de una
//! línea. Sin GPUI ni red: se prueba aparte.

/// Qué pasó con lo que se escribió.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineEvent {
    /// Enter: la línea completa.
    Submit(String),
    /// Ctrl+C, Ctrl+D con la línea vacía o Esc solo: el usuario no quiere responder.
    Cancel,
}

/// Editor de una línea. Con `secret` no muestra nada de lo escrito (contraseñas).
#[derive(Clone, Debug, Default)]
pub struct LineEditor {
    buf: String,
    secret: bool,
    /// Dentro de una secuencia de escape (flechas, respuestas del emulador): se ignora entera.
    in_escape: bool,
}

/// Largo máximo de lo que se acepta (una contraseña o "sí/no").
const MAX_LINE: usize = 1024;

impl LineEditor {
    pub fn new(secret: bool) -> Self {
        Self {
            secret,
            ..Self::default()
        }
    }

    /// Procesa teclas. Devuelve lo que hay que mostrar (eco) y, si terminó, el resultado.
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Option<LineEvent>) {
        let mut echo = Vec::new();
        let text = String::from_utf8_lossy(bytes);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if self.in_escape {
                // CSI/SS3: termina en una letra o en "~"; OSC y otros, sin importancia aquí.
                if c.is_ascii_alphabetic() || c == '~' {
                    self.in_escape = false;
                }
                continue;
            }
            match c {
                '\r' | '\n' => {
                    echo.extend_from_slice(b"\r\n");
                    return (echo, Some(LineEvent::Submit(std::mem::take(&mut self.buf))));
                }
                '\u{3}' => {
                    echo.extend_from_slice(b"^C\r\n");
                    return (echo, Some(LineEvent::Cancel));
                }
                '\u{4}' if self.buf.is_empty() => {
                    echo.extend_from_slice(b"\r\n");
                    return (echo, Some(LineEvent::Cancel));
                }
                '\u{1b}' => match chars.peek() {
                    // Esc solo (la tecla) cancela; Esc + "[" u "O" es una secuencia.
                    Some('[') | Some('O') | Some(']') => {
                        chars.next();
                        self.in_escape = true;
                    }
                    _ => {
                        echo.extend_from_slice(b"\r\n");
                        return (echo, Some(LineEvent::Cancel));
                    }
                },
                '\u{7f}' | '\u{8}' => {
                    if self.buf.pop().is_some() && !self.secret {
                        echo.extend_from_slice(b"\x08 \x08");
                    }
                }
                // Ctrl+U: borra toda la línea.
                '\u{15}' => {
                    let n = self.buf.chars().count();
                    self.buf.clear();
                    if !self.secret {
                        for _ in 0..n {
                            echo.extend_from_slice(b"\x08 \x08");
                        }
                    }
                }
                c if c.is_control() => {}
                c => {
                    if self.buf.len() < MAX_LINE {
                        self.buf.push(c);
                        if !self.secret {
                            let mut tmp = [0u8; 4];
                            echo.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
                        }
                    }
                }
            }
        }
        (echo, None)
    }
}

/// ¿La respuesta es "sí"? Acepta s, si, sí, y, yes (sin distinguir mayúsculas).
pub fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "s" | "si" | "sí" | "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribe_borra_y_envia() {
        let mut e = LineEditor::new(false);
        let (echo, ev) = e.feed(b"hola");
        assert_eq!(echo, b"hola");
        assert_eq!(ev, None);
        let (echo, _) = e.feed(b"\x7f");
        assert_eq!(echo, b"\x08 \x08");
        let (_, ev) = e.feed("é\r".as_bytes());
        assert_eq!(ev, Some(LineEvent::Submit("holé".into())));
        // Después de enviar, empieza vacía.
        let (_, ev) = e.feed(b"\r");
        assert_eq!(ev, Some(LineEvent::Submit(String::new())));
    }

    #[test]
    fn el_modo_secreto_no_muestra_nada() {
        let mut e = LineEditor::new(true);
        let (echo, _) = e.feed(b"clave\x7fs");
        assert!(echo.is_empty());
        let (echo, ev) = e.feed(b"\r");
        assert_eq!(echo, b"\r\n");
        assert_eq!(ev, Some(LineEvent::Submit("clavs".into())));
    }

    #[test]
    fn cancelar_con_ctrl_c_esc_o_ctrl_d() {
        assert_eq!(LineEditor::new(true).feed(b"ab\x03").1, Some(LineEvent::Cancel));
        assert_eq!(LineEditor::new(false).feed(b"\x1b").1, Some(LineEvent::Cancel));
        assert_eq!(LineEditor::new(false).feed(b"\x04").1, Some(LineEvent::Cancel));
        // Ctrl+D con texto no cancela.
        assert_eq!(LineEditor::new(false).feed(b"a\x04").1, None);
    }

    #[test]
    fn ignora_flechas_y_respuestas_del_emulador() {
        let mut e = LineEditor::new(false);
        // Flecha arriba, respuesta de posición del cursor y F5.
        let (echo, ev) = e.feed(b"\x1b[A\x1b[12;40R\x1b[15~s");
        assert_eq!(echo, b"s");
        assert_eq!(ev, None);
        // Secuencia partida entre lecturas.
        e.feed(b"\x1b[1;");
        let (_, ev) = e.feed(b"5Ci\r");
        assert_eq!(ev, Some(LineEvent::Submit("si".into())));
    }

    #[test]
    fn ctrl_u_borra_la_linea() {
        let mut e = LineEditor::new(false);
        e.feed(b"abc");
        let (echo, _) = e.feed(b"\x15");
        assert_eq!(echo, b"\x08 \x08\x08 \x08\x08 \x08");
        assert_eq!(e.feed(b"z\r").1, Some(LineEvent::Submit("z".into())));
    }

    #[test]
    fn respuestas_afirmativas() {
        for s in ["s", "Si", "SÍ", " yes ", "y"] {
            assert!(is_yes(s), "{s}");
        }
        for s in ["", "n", "no", "nope", "sii"] {
            assert!(!is_yes(s), "{s}");
        }
    }
}

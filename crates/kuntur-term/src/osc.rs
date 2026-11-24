//! Escáner de secuencias OSC (`ESC ] … BEL` o `ESC ] … ESC \`) en el flujo de salida del programa,
//! antes de que llegue al emulador. No altera los bytes; guarda estado porque una secuencia puede
//! partirse entre lecturas. Es el único escáner de la salida: lo usan la carpeta actual (OSC 7 /
//! 9;9), las marcas de integración de shell (OSC 133) y el estado de agentes (OSC 7501); ver
//! [`crate::cwd::OscEvent`].

/// Largo máximo de un OSC que se guarda; los más largos (p. ej. OSC 52 con un portapapeles grande)
/// se ignoran sin acumular memoria.
const MAX_OSC: usize = 4096;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    /// Se vio `ESC`.
    Esc,
    /// Dentro de `ESC ]`, guardando el contenido.
    Osc,
    /// Dentro de un OSC se vio `ESC`: si sigue `\` es el terminador ST.
    OscEsc,
}

#[derive(Default)]
pub struct OscScanner {
    state: State,
    buf: Vec<u8>,
    overflow: bool,
}

impl OscScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Procesa un trozo de salida y llama a `on_osc` con el contenido de cada OSC completo (sin
    /// `ESC ]` ni terminador), en orden.
    pub fn feed(&mut self, bytes: &[u8], mut on_osc: impl FnMut(&[u8])) {
        self.feed_at(bytes, |_, payload| on_osc(payload));
    }

    /// Como [`feed`](Self::feed), pero además entrega la posición del trozo **justo después** del
    /// terminador de cada OSC (para alimentar el emulador hasta ahí y saber dónde quedó el cursor).
    pub fn feed_at(&mut self, bytes: &[u8], mut on_osc: impl FnMut(usize, &[u8])) {
        for (i, &b) in bytes.iter().enumerate() {
            match self.state {
                State::Ground => {
                    if b == ESC {
                        self.state = State::Esc;
                    }
                }
                State::Esc => self.after_esc(b),
                State::Osc => match b {
                    BEL => {
                        self.finish(i + 1, &mut on_osc);
                        self.state = State::Ground;
                    }
                    ESC => self.state = State::OscEsc,
                    _ if self.buf.len() < MAX_OSC => self.buf.push(b),
                    _ => self.overflow = true,
                },
                State::OscEsc => {
                    if b == b'\\' {
                        self.finish(i + 1, &mut on_osc);
                        self.state = State::Ground;
                    } else {
                        // OSC interrumpido: ese ESC empieza otra secuencia.
                        self.after_esc(b);
                    }
                }
            }
        }
    }

    fn after_esc(&mut self, b: u8) {
        self.state = match b {
            b']' => {
                self.buf.clear();
                self.overflow = false;
                State::Osc
            }
            ESC => State::Esc,
            _ => State::Ground,
        };
    }

    fn finish(&mut self, end: usize, on_osc: &mut impl FnMut(usize, &[u8])) {
        if !self.overflow {
            on_osc(end, &self.buf);
        }
    }
}

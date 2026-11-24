//! Carpeta actual del shell a partir de las secuencias que emiten los shells:
//! - OSC 7: `ESC ] 7 ; file://host/C:/ruta` (estándar de facto; bash/zsh/fish, ruta en URL).
//! - OSC 9;9: `ESC ] 9 ; 9 ; "C:\ruta"` (Windows Terminal / ConEmu; PowerShell, cmd con `prompt`).
//!
//! Y además las marcas de integración de shell OSC 133 (`A` prompt, `B` entrada, `C` comando,
//! `D;código` fin), ver [`crate::shell`].
//!
//! Terminadas en BEL (`0x07`) o ST (`ESC \`). El escáner no altera los bytes: solo mira el flujo
//! antes de que llegue al emulador, y guarda estado porque una secuencia puede partirse entre lecturas.

use crate::shell::ShellMark;

/// Algo que el shell informó por OSC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OscEvent {
    /// Carpeta actual (OSC 7 / OSC 9;9).
    Cwd(String),
    /// Marca de integración de shell (OSC 133).
    Mark(ShellMark),
}

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
pub struct CwdScanner {
    state: State,
    buf: Vec<u8>,
    overflow: bool,
}

impl CwdScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Procesa un trozo de salida y devuelve la última carpeta completa encontrada en él.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<String> {
        self.feed_events(bytes).into_iter().rev().find_map(|(_, e)| match e {
            OscEvent::Cwd(d) => Some(d),
            OscEvent::Mark(_) => None,
        })
    }

    /// Procesa un trozo de salida y devuelve cada aviso con la posición del trozo **justo después**
    /// de su terminador (para alimentar el emulador hasta ahí y saber dónde quedó el cursor).
    pub fn feed_events(&mut self, bytes: &[u8]) -> Vec<(usize, OscEvent)> {
        let mut found = Vec::new();
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
                        found.extend(self.finish().map(|e| (i + 1, e)));
                        self.state = State::Ground;
                    }
                    ESC => self.state = State::OscEsc,
                    _ if self.buf.len() < MAX_OSC => self.buf.push(b),
                    _ => self.overflow = true,
                },
                State::OscEsc => {
                    if b == b'\\' {
                        found.extend(self.finish().map(|e| (i + 1, e)));
                        self.state = State::Ground;
                    } else {
                        // OSC interrumpido: ese ESC empieza otra secuencia.
                        self.after_esc(b);
                    }
                }
            }
        }
        found
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

    fn finish(&mut self) -> Option<OscEvent> {
        if self.overflow {
            return None;
        }
        let payload = String::from_utf8_lossy(&self.buf);
        if let Some(mark) = payload.strip_prefix("133;").and_then(ShellMark::parse) {
            return Some(OscEvent::Mark(mark));
        }
        let path = if let Some(rest) = payload.strip_prefix("9;9;") {
            let rest = rest.trim();
            let unquoted = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(rest);
            Some(unquoted.to_string())
        } else if let Some(url) = payload.strip_prefix("7;") {
            path_from_file_url(url.trim())
        } else {
            None
        };
        path.filter(|p| !p.is_empty()).map(OscEvent::Cwd)
    }
}

/// `file://host/C:/ruta%20x` → `C:\ruta x`; `file://host/home/x` → `/home/x`.
fn path_from_file_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let path = percent_decode(path);
    let bytes = path.as_bytes();
    let windows = bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    if windows {
        let mut p = path[1..].replace('/', "\\");
        if p.len() == 2 {
            p.push('\\');
        }
        Some(p)
    } else {
        Some(path)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(chunks: &[&[u8]]) -> Option<String> {
        let mut s = CwdScanner::new();
        let mut last = None;
        for c in chunks {
            if let Some(p) = s.feed(c) {
                last = Some(p);
            }
        }
        last
    }

    #[test]
    fn osc_9_9_con_bel_y_con_st() {
        assert_eq!(
            scan(&[b"PS> \x1b]9;9;\"C:\\Users\\demo\"\x07PS C:\\> "]).as_deref(),
            Some(r"C:\Users\demo")
        );
        assert_eq!(scan(&[b"\x1b]9;9;\"D:\\a b\"\x1b\\resto"]).as_deref(), Some(r"D:\a b"));
        // Sin comillas también vale.
        assert_eq!(scan(&[b"\x1b]9;9;C:\\Windows\x07"]).as_deref(), Some(r"C:\Windows"));
    }

    #[test]
    fn osc_7_con_url_file() {
        assert_eq!(
            scan(&[b"\x1b]7;file://equipo/C:/Users/demo/mi%20carpeta\x07"]).as_deref(),
            Some(r"C:\Users\demo\mi carpeta")
        );
        assert_eq!(
            scan(&[b"\x1b]7;file://equipo/home/demo\x1b\\"]).as_deref(),
            Some("/home/demo")
        );
        assert_eq!(
            scan(&[b"\x1b]7;file:///C:/x/%C3%B1\x07"]).as_deref(),
            Some("C:\\x\\\u{f1}")
        );
    }

    #[test]
    fn secuencia_partida_entre_lecturas() {
        assert_eq!(
            scan(&[b"hola \x1b]9;9;\"C:\\pro", b"yecto\"\x1b", b"\\ fin"]).as_deref(),
            Some(r"C:\proyecto")
        );
        assert_eq!(
            scan(&[b"\x1b", b"]", b"7;file://h/C:/a", b"\x07"]).as_deref(),
            Some(r"C:\a")
        );
    }

    #[test]
    fn devuelve_la_ultima_y_ignora_otros_osc() {
        let mut s = CwdScanner::new();
        assert_eq!(s.feed(b"\x1b]0;titulo\x07texto\x1b[31mrojo"), None);
        assert_eq!(
            s.feed(b"\x1b]9;9;\"C:\\uno\"\x07\x1b]9;9;\"C:\\dos\"\x07").as_deref(),
            Some(r"C:\dos")
        );
        assert_eq!(s.feed(b"\x1b]9;4;1;50\x07"), None, "OSC 9;4 es progreso, no carpeta");
        // Un OSC sin terminar interrumpido por otra secuencia no deja estado roto.
        assert_eq!(
            s.feed(b"\x1b]9;9;C:\\rota\x1b[0m\x1b]9;9;C:\\bien\x07").as_deref(),
            Some(r"C:\bien")
        );
        // Un OSC enorme se descarta sin acumular.
        let mut grande = b"\x1b]52;c;".to_vec();
        grande.extend(std::iter::repeat_n(b'A', 100_000));
        grande.push(0x07);
        assert_eq!(s.feed(&grande), None);
        assert_eq!(s.feed(b"\x1b]9;9;C:\\despues\x07").as_deref(), Some(r"C:\despues"));
        assert_eq!(s.feed(b"\x1b]9;9;\x07"), None, "una ruta vacía no cuenta");
    }

    #[test]
    fn marcas_osc_133_con_su_posicion() {
        let (d, a, prompt, b) = (
            &b"\x1b]133;D;1\x07"[..],
            &b"\x1b]133;A\x1b\\"[..],
            &b"PS C:\\> "[..],
            &b"\x1b]133;B\x07"[..],
        );
        let chunk = [d, a, prompt, b].concat();
        let mut s = CwdScanner::new();
        assert_eq!(
            s.feed_events(&chunk),
            vec![
                (d.len(), OscEvent::Mark(ShellMark::CommandEnd(Some(1)))),
                (d.len() + a.len(), OscEvent::Mark(ShellMark::PromptStart)),
                (chunk.len(), OscEvent::Mark(ShellMark::InputStart)),
            ]
        );
        // `feed` sigue devolviendo solo carpetas.
        assert_eq!(s.feed(b"\x1b]133;C\x07"), None);
        // Partida entre lecturas: la posición es la del trozo donde termina.
        assert!(s.feed_events(b"\x1b]13").is_empty());
        assert_eq!(
            s.feed_events(b"3;B\x07xy"),
            vec![(4, OscEvent::Mark(ShellMark::InputStart))]
        );
    }
}

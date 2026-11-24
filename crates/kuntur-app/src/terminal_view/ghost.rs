//! Texto fantasma (autocompletado estilo fish/Warp): mientras el usuario escribe en el prompt, el
//! resto sugerido se dibuja en gris después del cursor; **Tab** o **→** lo acepta (Kuntur escribe en
//! el shell solo lo que falta) y **Esc** lo oculta. Nada se escribe en el shell hasta aceptar.
//!
//! Solo aparece si el shell marcó con OSC 133 que está esperando entrada (no dentro de `claude`,
//! `codex`, `vim`… ni en pantalla alterna) y si el cursor está al final de lo escrito.
//! El motor (rutas, historial, memoria de carpetas) está en `kuntur-complete`.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::*;
use kuntur_complete::{Completer, Memory, Suggestion};
use kuntur_term::{Key, Mods, Pane, encode_key};

use super::Metrics;

/// Motor compartido por todas las terminales (historial y carpetas son uno solo). `None` si el
/// autocompletado está apagado en `config.toml`.
static COMPLETER: OnceLock<Mutex<Option<Completer>>> = OnceLock::new();

/// Se llama una vez al arrancar, con `autocompletado` de la configuración.
///
/// En la prueba de humo (`KUNTUR_SMOKE_INPUT`) no se toca el historial real: se usa uno en memoria
/// con los comandos sintéticos de `KUNTUR_SMOKE_HISTORY` ("\n" literal los separa).
pub fn init(enabled: bool) {
    let completer = enabled.then(|| {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let smoke = std::env::var_os("KUNTUR_SMOKE_INPUT").is_some();
        let memory = match Memory::default_dir() {
            Some(dir) if !smoke => Memory::open(&dir),
            _ => Memory::in_memory(),
        };
        let mut c = Completer::new(memory, home);
        if smoke && let Ok(list) = std::env::var("KUNTUR_SMOKE_HISTORY") {
            for (i, cmd) in list.split("\\n").enumerate() {
                c.memory.history.push(cmd, "", i as u64);
            }
        }
        c
    });
    let _ = COMPLETER.set(Mutex::new(completer));
}

fn with_completer<R>(f: impl FnOnce(&mut Completer) -> R) -> Option<R> {
    let mut guard = COMPLETER.get()?.lock().ok()?;
    guard.as_mut().map(f)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Sugerencia visible ahora mismo.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// Lo escrito cuando se calculó (si cambia, se recalcula).
    pub input: String,
    pub col: usize,
    pub row: usize,
    pub suggestion: Suggestion,
}

/// Qué hacer con una tecla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Kuntur la usó (aceptar u ocultar): no va al shell.
    Handled,
    /// Sigue su camino normal al shell.
    Pass,
}

#[derive(Default)]
pub struct Ghost {
    shown: Option<Shown>,
    /// El usuario la ocultó con Esc para esta entrada: no vuelve hasta que cambie lo escrito.
    dismissed: Option<String>,
    last_cwd: Option<PathBuf>,
    /// Terminal remota (F6): las rutas, el historial y la memoria de carpetas son de esta PC, así que
    /// no se sugiere nada ni se guarda lo que se escribe en el servidor.
    remote: bool,
}

impl Ghost {
    /// Apaga las sugerencias para una terminal remota (SSH).
    pub fn set_remote(&mut self) {
        self.remote = true;
        self.shown = None;
    }

    pub fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }

    /// Hubo salida del shell: recordar la carpeta si cambió y recalcular la sugerencia.
    pub fn refresh(&mut self, pane: &Pane) {
        if self.remote {
            return;
        }
        let cwd = pane.cwd();
        if cwd.is_some() && cwd != self.last_cwd {
            if let Some(dir) = &cwd {
                with_completer(|c| c.record_visit(dir, now_secs()));
            }
            self.last_cwd = cwd.clone();
        }
        let Some(line) = pane.prompt_input().filter(|l| l.at_end) else {
            self.shown = None;
            return;
        };
        if self.dismissed.as_ref().is_some_and(|d| *d != line.text) {
            self.dismissed = None;
        }
        if self.dismissed.is_some() {
            self.shown = None;
            return;
        }
        if let Some(s) = &self.shown
            && s.input == line.text
            && (s.col, s.row) == (line.cursor_col, line.cursor_row)
        {
            return;
        }
        let suggestion = with_completer(|c| c.suggest(&line.text, cwd.as_deref(), now_secs())).flatten();
        self.shown = suggestion.map(|suggestion| Shown {
            input: line.text,
            col: line.cursor_col,
            row: line.cursor_row,
            suggestion,
        });
    }

    /// Tecla sin modificadores (`tab`, `right`, `escape`, `enter`) antes de mandarla al shell.
    pub fn on_key(&mut self, key: &str, pane: &Pane) -> KeyOutcome {
        match key {
            "tab" | "right" => {
                let Some(shown) = self.shown.take() else {
                    return KeyOutcome::Pass;
                };
                // Solo si lo escrito sigue igual (la salida del shell pudo cambiar algo).
                let still = pane.prompt_input().is_some_and(|l| l.at_end && l.text == shown.input);
                if !still {
                    return KeyOutcome::Pass;
                }
                let s = &shown.suggestion;
                let mut bytes = Vec::new();
                if let Some(bs) = encode_key(&Key::Backspace, Mods::NONE, false) {
                    for _ in 0..s.erase {
                        bytes.extend_from_slice(&bs);
                    }
                }
                bytes.extend_from_slice(s.insert.as_bytes());
                let _ = pane.write(&bytes);
                KeyOutcome::Handled
            }
            "escape" => match self.shown.take() {
                Some(shown) => {
                    self.dismissed = Some(shown.input);
                    KeyOutcome::Handled
                }
                None => KeyOutcome::Pass,
            },
            "enter" => {
                self.shown = None;
                self.dismissed = None;
                if !self.remote
                    && let Some(line) = pane.prompt_input()
                {
                    let cwd = pane.cwd();
                    with_completer(|c| c.record_command(&line.text, cwd.as_deref(), now_secs()));
                }
                pane.mark_command_start();
                KeyOutcome::Pass
            }
            _ => KeyOutcome::Pass,
        }
    }
}

/// Prueba de humo: escribe `KUNTUR_SMOKE_TYPE` (sin Enter) para capturar el texto fantasma.
pub fn smoke_type(pane: &Pane) {
    if let Ok(text) = std::env::var("KUNTUR_SMOKE_TYPE") {
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let _ = pane.write(text.as_bytes());
    }
}

/// Fuente y color del texto fantasma.
pub struct GhostStyle {
    pub family: SharedString,
    pub size: Pixels,
    pub color: Hsla,
}

/// Dibuja la sugerencia en gris a partir del cursor, sin pasar del borde derecho.
pub(super) fn paint(
    shown: &Shown,
    cols: usize,
    bounds: Bounds<Pixels>,
    m: Metrics,
    style: &GhostStyle,
    window: &mut Window,
    cx: &mut App,
) {
    let room = cols.saturating_sub(shown.col);
    let text: String = shown.suggestion.ghost.chars().take(room).collect();
    if text.is_empty() {
        return;
    }
    let run = TextRun {
        len: text.len(),
        font: font(style.family.clone()),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text.into(), style.size, &[run], Some(m.cell_w));
    let origin = point(
        bounds.origin.x + m.cell_w * shown.col as f32,
        bounds.origin.y + m.line_h * shown.row as f32,
    );
    let _ = line.paint(origin, m.line_h, TextAlign::Left, None, window, cx);
}

#[cfg(all(test, windows))]
mod tests {
    // Sin `use super::*`: gpui-kit exporta su propio `test`.
    use super::{COMPLETER, Ghost, KeyOutcome, with_completer};
    use kuntur_complete::{Completer, Memory};
    use kuntur_term::{GridSize, Pane, PaneOptions};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// De punta a punta con Windows PowerShell 5.1 real: escribir una ruta parcial muestra el resto
    /// en gris, Esc la oculta, Tab escribe en el shell solo lo que falta y Enter guarda el comando.
    #[test]
    fn tab_acepta_la_ruta_sugerida_en_powershell() {
        let _ = COMPLETER.set(Mutex::new(Some(Completer::new(Memory::in_memory(), None))));
        let base = std::env::temp_dir().join(format!("kuntur-ghost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("proyecto-demo").join("Datos de prueba")).unwrap();

        let mut opts = PaneOptions::default_shell(GridSize::new(160, 30));
        opts.program = "powershell.exe".into();
        opts.args.insert(1, "-NoProfile".into());
        opts.cwd = Some(base.clone());
        let pane = Pane::spawn(opts, || {}).unwrap();
        let mut ghost = Ghost::default();
        assert!(wait_until(|| pane.prompt_input().is_some()));

        pane.write(b"cd proy").unwrap();
        assert!(
            wait_until(|| {
                ghost.refresh(&pane);
                ghost.shown().is_some()
            }),
            "no apareció la sugerencia: {:?}",
            pane.prompt_input()
        );
        assert_eq!(ghost.shown().unwrap().suggestion.ghost, "ecto-demo\\");
        // Esc la oculta (y no llega al shell); vuelve al cambiar lo escrito.
        assert_eq!(ghost.on_key("escape", &pane), KeyOutcome::Handled);
        ghost.refresh(&pane);
        assert!(ghost.shown().is_none());
        pane.write(b"e").unwrap();
        assert!(wait_until(|| {
            ghost.refresh(&pane);
            ghost.shown().is_some_and(|s| s.suggestion.ghost == "cto-demo\\")
        }));

        assert_eq!(ghost.on_key("tab", &pane), KeyOutcome::Handled);
        assert!(wait_until(|| pane
            .prompt_input()
            .is_some_and(|l| l.text == "cd proyecto-demo\\")));
        // Sin sugerencia, Tab sigue al shell.
        ghost.refresh(&pane);
        assert_eq!(ghost.on_key("tab", &pane), KeyOutcome::Pass);

        assert_eq!(ghost.on_key("enter", &pane), KeyOutcome::Pass);
        pane.write(b"\r").unwrap();
        assert!(wait_until(|| pane.cwd().is_some_and(|d| d.ends_with("proyecto-demo"))));
        let saved = with_completer(|c| c.memory.history.entries().last().map(|e| e.command.clone()));
        assert_eq!(saved.flatten().as_deref(), Some("cd proyecto-demo\\"));

        // Ruta con espacios: Tab borra la palabra escrita y la reescribe entre comillas simples,
        // así PowerShell no la corta en el espacio.
        assert!(wait_until(|| pane.prompt_input().is_some_and(|l| l.text.is_empty())));
        pane.write(b"cd Datos").unwrap();
        assert!(
            wait_until(|| {
                ghost.refresh(&pane);
                ghost.shown().is_some_and(|s| s.input == "cd Datos")
            }),
            "no apareció la sugerencia con espacios: {:?}",
            pane.prompt_input()
        );
        assert_eq!(ghost.shown().unwrap().suggestion.ghost, "  → 'Datos de prueba\\'");
        assert_eq!(ghost.on_key("tab", &pane), KeyOutcome::Handled);
        assert!(
            wait_until(|| pane.prompt_input().is_some_and(|l| l.text == "cd 'Datos de prueba\\'")),
            "quedó: {:?}",
            pane.prompt_input()
        );
        assert_eq!(ghost.on_key("enter", &pane), KeyOutcome::Pass);
        pane.write(b"\r").unwrap();
        assert!(wait_until(|| pane
            .cwd()
            .is_some_and(|d| d.ends_with("Datos de prueba"))));
    }
}

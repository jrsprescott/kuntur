//! `kuntur-hook.exe`: lo invocan los hooks de Claude Code (`kuntur-hook`) y su línea de estado
//! (`kuntur-hook statusline`). Lee el JSON de stdin, arma un evento compacto y lo manda al named pipe
//! de Kuntur (`KUNTUR_PIPE`) con la terminal de origen (`KUNTUR_SESSION`).
//!
//! Reglas: rápido, nunca bloquea al agente (si Kuntur no está, vuelve enseguida) y nunca falla de
//! forma visible: siempre sale con 0, sin escribir en stderr, y como gancho no escribe nada en stdout
//! (en `UserPromptSubmit` lo escrito se agregaría al contexto del agente).

use std::io::{Read, Write};

use kuntur_hook::{ENV_PIPE, ENV_SESSION, event, send};
use serde_json::Value;

/// Entrada máxima que se lee (un `Write` grande trae el archivo entero en `tool_input`).
const MAX_INPUT: u64 = 8 * 1024 * 1024;

fn read_input() -> Option<Value> {
    let mut buf = Vec::new();
    std::io::stdin().lock().take(MAX_INPUT).read_to_end(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn target() -> Option<(String, String)> {
    let pipe = std::env::var(ENV_PIPE).ok().filter(|p| !p.is_empty())?;
    let pane = std::env::var(ENV_SESSION).ok().filter(|p| !p.is_empty())?;
    Some((pipe, pane))
}

fn run() {
    let statusline = std::env::args()
        .nth(1)
        .is_some_and(|a| a.eq_ignore_ascii_case("statusline"));
    let input = read_input();
    if statusline {
        // La línea de estado se muestra aunque Kuntur no esté.
        let text = input.as_ref().map(event::statusline_text).unwrap_or_default();
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    }
    let (Some(input), Some((pipe, pane))) = (input, target()) else {
        return;
    };
    let ev = if statusline {
        event::from_statusline(&input, &pane)
    } else {
        event::from_claude(&input, &pane)
    };
    if let Some(ev) = ev {
        let _ = send(&pipe, &ev.encode());
    }
}

fn main() {
    // Ni un pánico debe llegar al agente (ni su mensaje a stderr).
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(run);
    std::process::exit(0);
}

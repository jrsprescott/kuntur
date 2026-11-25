//! Auditoría (§13): los comandos que ejecutaron los agentes, leídos de sus registros locales
//! (Claude Code y Codex). Sin UI: la app lo llama en un hilo aparte.
//!
//! Cada agente necesita su lector: solo hay lectores para formatos verificados. Los hooks de F5b
//! sumarán los comandos de cualquier agente con hook.

pub mod cache;
pub mod claude;
pub mod codex;
pub mod export;
pub mod filter;
pub mod model;
pub mod synthetic;

pub use cache::{AuditCache, Sources};
pub use filter::{AuditFilter, filter, period_start};
pub use kuntur_usage::{AgentKind, Period};
pub use kuntur_usage::{WINDOW_DAYS, window_start};
pub use model::{AuditRecord, LOCAL_HOST, dedup};

use std::path::{Path, PathBuf};

/// Archivos bajo `root` (recursivo) cuyo nombre termina en alguno de `suffixes`, en orden estable.
/// Una carpeta inexistente o ilegible da una lista vacía.
pub(crate) fn files_under(root: &Path, suffixes: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => pending.push(path),
                Ok(t) if t.is_file() => {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if suffixes.iter().any(|s| name.ends_with(s)) {
                        out.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// Llama a `f` con cada línea de `reader`, de a una y con un mismo búfer: nunca tiene el archivo
/// entero en memoria (los historiales de Claude Code pasan de varios MB). Una línea que no es UTF-8
/// válido se lee con reemplazos; la lectura termina en el primer error de E/S.
pub(crate) fn for_each_line(mut reader: impl std::io::BufRead, mut f: impl FnMut(&str)) {
    let mut buf = Vec::with_capacity(64 * 1024);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf);
        f(line.trim_end_matches(['\n', '\r']));
    }
}

/// Segundos Unix de una fecha ISO 8601 (`2026-11-20T10:00:00.000Z`).
pub(crate) fn parse_iso(s: &str) -> Option<i64> {
    s.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

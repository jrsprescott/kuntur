//! Uso de los agentes de código: lectores del historial local (Claude Code, Codex, OpenCode) y
//! agregaciones por período para la vista "Uso de agentes". Sin UI: la app lo llama en un hilo aparte.

pub mod aggregate;
pub mod claude;
pub mod codex;
pub mod model;

pub use aggregate::{ModelRow, Summary, TaskRow, Totals, pct_change, provider_of, summarize};
pub use model::{AgentKind, Period, UsageRecord};

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

/// Segundos Unix de una fecha ISO 8601 (`2026-11-20T10:00:00.000Z`).
pub(crate) fn parse_iso(s: &str) -> Option<i64> {
    s.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

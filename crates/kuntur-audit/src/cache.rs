//! Caché incremental de la auditoría, en memoria y por archivo (mismo modelo que el almacén de
//! `kuntur-usage`): cada archivo de origen se guarda con su huella (tamaño + fecha de modificación) y
//! al refrescar solo se releen los nuevos o cambiados; los que desaparecieron se olvidan. La app
//! conserva la caché mientras está abierta, así reabrir "Auditoría" no relee todo el historial.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::model::{AuditRecord, dedup};
use crate::{claude, codex};

/// Dónde están los registros de cada agente.
#[derive(Clone, Debug)]
pub struct Sources {
    pub claude_root: PathBuf,
    pub codex_root: PathBuf,
}

impl Sources {
    /// Las ubicaciones normales bajo la carpeta personal.
    pub fn under_home(home: &Path) -> Self {
        Self {
            claude_root: home.join(".claude").join("projects"),
            codex_root: home.join(".codex").join("sessions"),
        }
    }
}

/// Qué hizo un refresco.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshStats {
    pub scanned: usize,
    pub reread: usize,
    pub removed: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Claude,
    Codex,
}

type Fingerprint = (u64, u128);

fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Some((meta.len(), modified))
}

#[derive(Default)]
pub struct AuditCache {
    files: HashMap<PathBuf, (Fingerprint, Vec<AuditRecord>)>,
}

impl AuditCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Relee lo nuevo o cambiado de `sources` y olvida lo que ya no existe.
    pub fn refresh(&mut self, sources: &Sources) -> RefreshStats {
        let units: Vec<(PathBuf, Kind)> = claude::files(&sources.claude_root)
            .into_iter()
            .map(|p| (p, Kind::Claude))
            .chain(codex::files(&sources.codex_root).into_iter().map(|p| (p, Kind::Codex)))
            .collect();
        let mut stats = RefreshStats {
            scanned: units.len(),
            ..RefreshStats::default()
        };
        let mut seen = HashSet::with_capacity(units.len());
        for (path, kind) in units {
            let Some(fp) = fingerprint(&path) else { continue };
            seen.insert(path.clone());
            if self.files.get(&path).is_some_and(|(known, _)| *known == fp) {
                continue;
            }
            let records = match kind {
                Kind::Claude => claude::read_file(&path),
                Kind::Codex => codex::read_file(&path),
            };
            stats.reread += 1;
            self.files.insert(path, (fp, records));
        }
        let before = self.files.len();
        self.files.retain(|p, _| seen.contains(p));
        stats.removed = before - self.files.len();
        stats
    }

    /// Todos los comandos, sin repetidos y del más nuevo al más viejo.
    pub fn records(&self) -> Vec<AuditRecord> {
        dedup(self.files.values().flat_map(|(_, r)| r.iter().cloned()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:00:05.000Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}]}}
"#;
    const CLAUDE_MORE: &str = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:00:05.000Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}]}}
{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:05:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"pwd"}}]}}
"#;
    const CODEX: &str = r#"{"timestamp":"2026-11-21T09:00:00.000Z","type":"response_item","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"cargo build\"}","call_id":"c1"}}
"#;

    #[test]
    fn refresco_incremental() {
        let home = std::env::temp_dir().join(format!("kuntur-audit-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let sources = Sources::under_home(&home);
        let proj = sources.claude_root.join("proyecto");
        let day = sources.codex_root.join("2026").join("11").join("21");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(proj.join("a.jsonl"), CLAUDE).unwrap();
        // Copia de la sesión reanudada: mismo `tool_use.id`.
        std::fs::write(proj.join("b.jsonl"), CLAUDE).unwrap();
        std::fs::write(day.join("rollout-1.jsonl"), CODEX).unwrap();

        let mut cache = AuditCache::new();
        let first = cache.refresh(&sources);
        let r1 = cache.records();
        // Nada cambió: no relee nada.
        let second = cache.refresh(&sources);
        // Cambia un archivo y se borra otro.
        std::fs::write(proj.join("a.jsonl"), CLAUDE_MORE).unwrap();
        std::fs::remove_file(day.join("rollout-1.jsonl")).unwrap();
        let third = cache.refresh(&sources);
        let r3 = cache.records();
        let _ = std::fs::remove_dir_all(&home);

        assert_eq!(
            first,
            RefreshStats {
                scanned: 3,
                reread: 3,
                removed: 0
            }
        );
        let cmds: Vec<&str> = r1.iter().map(|r| r.command.as_str()).collect();
        assert_eq!(cmds, ["cargo build", "ls"]);
        assert_eq!(
            second,
            RefreshStats {
                scanned: 3,
                reread: 0,
                removed: 0
            }
        );
        assert_eq!(
            third,
            RefreshStats {
                scanned: 2,
                reread: 1,
                removed: 1
            }
        );
        let cmds: Vec<&str> = r3.iter().map(|r| r.command.as_str()).collect();
        assert_eq!(cmds, ["pwd", "ls"]);
    }

    #[test]
    fn sin_carpetas_no_falla() {
        let mut cache = AuditCache::new();
        let stats = cache.refresh(&Sources::under_home(Path::new("Z:/no/existe")));
        assert_eq!(stats, RefreshStats::default());
        assert!(cache.records().is_empty());
    }
}

//! Caché incremental de la auditoría, en memoria y por archivo (mismo modelo que el almacén de
//! `kuntur-usage`): cada archivo de origen se guarda con su huella (tamaño + fecha de modificación) y
//! al refrescar solo se releen los nuevos o cambiados; los que desaparecieron se olvidan. La app
//! conserva la caché mientras está abierta, así reabrir "Auditoría" no relee todo el historial.
//!
//! Solo guarda la ventana de los últimos 30 días (`since`): un archivo que no cambió desde antes
//! no se abre, y los comandos que van quedando fuera se sueltan. Los comandos se guardan en `Arc`:
//! la lista que muestra la vista los comparte con la caché en vez de copiarlos.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// Segundos Unix de la última modificación de una huella.
fn modified_secs(fp: Fingerprint) -> i64 {
    i64::try_from(fp.1 / 1_000_000_000).unwrap_or(i64::MAX)
}

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
    files: HashMap<PathBuf, (Fingerprint, Vec<Arc<AuditRecord>>)>,
    /// Huella del índice de títulos de Codex: si cambia, se releen sus rollouts (cambió un título).
    codex_index: Option<Fingerprint>,
}

impl AuditCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Relee lo nuevo o cambiado de `sources` desde `since` (segundos Unix) y olvida lo que ya no
    /// existe o quedó fuera de la ventana.
    pub fn refresh(&mut self, sources: &Sources, since: i64) -> RefreshStats {
        let units: Vec<(PathBuf, Kind)> = claude::files(&sources.claude_root)
            .into_iter()
            .map(|p| (p, Kind::Claude))
            .chain(codex::files(&sources.codex_root).into_iter().map(|p| (p, Kind::Codex)))
            .collect();
        let index = kuntur_usage::codex::titles_path(&sources.codex_root).and_then(|p| fingerprint(&p));
        let titles_changed = index != self.codex_index;
        self.codex_index = index;
        // Solo se leen si hay rollouts de Codex que releer.
        let mut codex_titles: Option<HashMap<String, String>> = None;
        let mut stats = RefreshStats {
            scanned: units.len(),
            ..RefreshStats::default()
        };
        let mut seen = HashSet::with_capacity(units.len());
        for (path, kind) in units {
            let Some(fp) = fingerprint(&path) else { continue };
            // Sin cambios desde antes de la ventana: no puede traer comandos de la ventana.
            if modified_secs(fp) < since {
                continue;
            }
            seen.insert(path.clone());
            let stale = kind == Kind::Codex && titles_changed;
            if !stale
                && let Some((known, records)) = self.files.get_mut(&path)
                && *known == fp
            {
                records.retain(|r| r.ts >= since);
                continue;
            }
            let records = match kind {
                Kind::Claude => claude::read_file(&path, since),
                Kind::Codex => {
                    let titles = codex_titles.get_or_insert_with(|| kuntur_usage::codex::titles(&sources.codex_root));
                    codex::read_file(&path, since, titles)
                }
            };
            stats.reread += 1;
            self.files
                .insert(path, (fp, records.into_iter().map(Arc::new).collect()));
        }
        let before = self.files.len();
        self.files.retain(|p, _| seen.contains(p));
        stats.removed = before - self.files.len();
        self.files.shrink_to_fit();
        stats
    }

    /// Todos los comandos, sin repetidos y del más nuevo al más viejo (comparten memoria con la caché).
    pub fn records(&self) -> Vec<Arc<AuditRecord>> {
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
        let first = cache.refresh(&sources, i64::MIN);
        let r1 = cache.records();
        // Nada cambió: no relee nada.
        let second = cache.refresh(&sources, i64::MIN);
        // Cambia un archivo y se borra otro.
        std::fs::write(proj.join("a.jsonl"), CLAUDE_MORE).unwrap();
        std::fs::remove_file(day.join("rollout-1.jsonl")).unwrap();
        let third = cache.refresh(&sources, i64::MIN);
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
        let cmds: Vec<&str> = r1.iter().map(|r| &*r.command).collect();
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
        let cmds: Vec<&str> = r3.iter().map(|r| &*r.command).collect();
        assert_eq!(cmds, ["pwd", "ls"]);
    }

    #[test]
    fn sin_carpetas_no_falla() {
        let mut cache = AuditCache::new();
        let stats = cache.refresh(&Sources::under_home(Path::new("Z:/no/existe")), i64::MIN);
        assert_eq!(stats, RefreshStats::default());
        assert!(cache.records().is_empty());
    }

    #[test]
    fn fuera_de_la_ventana_no_se_lee_ni_se_guarda() {
        let home = std::env::temp_dir().join(format!("kuntur-audit-ventana-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let sources = Sources::under_home(&home);
        let proj = sources.claude_root.join("proyecto");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("a.jsonl"), CLAUDE_MORE).unwrap();
        let mut cache = AuditCache::new();
        // Ventana después de la última modificación del archivo: no se abre.
        let future = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap()
            + 3_600;
        let stats = cache.refresh(&sources, future);
        assert_eq!((stats.scanned, stats.reread), (1, 0));
        assert!(cache.records().is_empty());
        // Ventana que corta entre los dos comandos: solo queda el más nuevo, aun sin releer.
        cache.refresh(&sources, i64::MIN);
        assert_eq!(cache.records().len(), 2);
        let cut = crate::parse_iso("2026-11-20T10:01:00Z").unwrap();
        let stats = cache.refresh(&sources, cut);
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(stats.reread, 0);
        let records = cache.records();
        let cmds: Vec<&str> = records.iter().map(|r| &*r.command).collect();
        assert_eq!(cmds, ["pwd"]);
    }
}

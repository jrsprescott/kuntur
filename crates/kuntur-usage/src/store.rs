//! Almacén incremental del historial de uso (`%APPDATA%\Kuntur\usage.db`, SQLite).
//!
//! Cada archivo de origen (un JSONL de Claude Code, un rollout de Codex, la base de OpenCode) se
//! guarda con su "huella" (tamaño + fecha de modificación). Al importar, solo se releen los archivos
//! cuya huella cambió; los que desaparecieron se borran. Así, abrir "Uso de agentes" no vuelve a
//! leer todo el historial cada vez.
//!
//! - Las respuestas que Claude Code repite en varios archivos (al reanudar una sesión) se cuentan
//!   una vez al leer, por `key`.
//! - El costo estimado **no** se guarda: se calcula al leer con la tabla de precios vigente, así un
//!   cambio en `pricing.toml` se ve sin reimportar. Sí se guarda el costo real de OpenCode.
//! - Si cambia el esquema (`SCHEMA_VERSION`), se borra y se vuelve a importar todo.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::{AgentKind, Cost, UsageRecord, claude, codex, opencode};

const SCHEMA_VERSION: i32 = 1;

/// Dónde está el historial de cada agente.
#[derive(Clone, Debug)]
pub struct Sources {
    pub claude_root: PathBuf,
    pub codex_root: PathBuf,
    pub opencode_db: PathBuf,
}

impl Sources {
    /// Las ubicaciones normales bajo la carpeta personal.
    pub fn under_home(home: &Path) -> Self {
        Self {
            claude_root: home.join(".claude").join("projects"),
            codex_root: home.join(".codex").join("sessions"),
            opencode_db: home.join(".local").join("share").join("opencode").join("opencode.db"),
        }
    }
}

/// Qué hizo una importación.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportStats {
    /// Archivos de origen encontrados.
    pub scanned: usize,
    /// Archivos nuevos o cambiados que se volvieron a leer.
    pub reread: usize,
    /// Archivos que ya no existen (sus registros se borraron).
    pub removed: usize,
}

#[derive(Clone, Copy)]
enum Kind {
    Claude,
    Codex,
    OpenCode,
}

struct Unit {
    path: PathBuf,
    fingerprint: String,
    kind: Kind,
}

pub struct UsageStore {
    con: Connection,
}

impl UsageStore {
    /// Abre (o crea) la base en `path`.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    /// Base en memoria (pruebas).
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(con: Connection) -> rusqlite::Result<Self> {
        let version: i32 = con.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            con.execute_batch("DROP TABLE IF EXISTS records; DROP TABLE IF EXISTS files;")?;
        }
        con.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                 path TEXT PRIMARY KEY,
                 fingerprint TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS records (
                 id INTEGER PRIMARY KEY,
                 file TEXT NOT NULL,
                 agent INTEGER NOT NULL,
                 model TEXT NOT NULL,
                 ts INTEGER NOT NULL,
                 input INTEGER NOT NULL,
                 output INTEGER NOT NULL,
                 cache_read INTEGER NOT NULL,
                 cache_write INTEGER NOT NULL,
                 session_id TEXT NOT NULL,
                 title TEXT,
                 project TEXT,
                 real_cost INTEGER,
                 key TEXT
             );
             CREATE INDEX IF NOT EXISTS records_file ON records(file);",
        )?;
        con.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { con })
    }

    /// Importa lo nuevo o cambiado de `sources` y borra lo que ya no existe.
    pub fn import(&mut self, sources: &Sources) -> rusqlite::Result<ImportStats> {
        let units = units(sources);
        let codex_titles = codex::titles(&sources.codex_root);
        let known: HashMap<String, String> = {
            let mut stmt = self.con.prepare("SELECT path, fingerprint FROM files")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        let mut stats = ImportStats {
            scanned: units.len(),
            ..ImportStats::default()
        };
        let tx = self.con.transaction()?;
        let mut seen = HashSet::new();
        for unit in &units {
            let path = unit.path.to_string_lossy().into_owned();
            seen.insert(path.clone());
            if known.get(&path) == Some(&unit.fingerprint) {
                continue;
            }
            let records = match unit.kind {
                Kind::Claude => claude::read_file(&sources.claude_root, &unit.path),
                Kind::Codex => codex::read_file(&unit.path, &codex_titles),
                Kind::OpenCode => opencode::read_db(&unit.path),
            };
            tx.execute("DELETE FROM records WHERE file = ?1", [&path])?;
            {
                let mut insert = tx.prepare_cached(
                    "INSERT INTO records (file, agent, model, ts, input, output, cache_read, cache_write,
                                          session_id, title, project, real_cost, key)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                )?;
                for r in &records {
                    let real = match r.cost {
                        Cost::Real(m) => Some(m as i64),
                        _ => None,
                    };
                    insert.execute(params![
                        path,
                        agent_code(r.agent),
                        r.model,
                        r.ts,
                        r.input as i64,
                        r.output as i64,
                        r.cache_read as i64,
                        r.cache_write as i64,
                        r.session_id,
                        r.title,
                        r.project,
                        real,
                        r.key,
                    ])?;
                }
            }
            tx.execute(
                "INSERT OR REPLACE INTO files (path, fingerprint) VALUES (?1, ?2)",
                params![path, unit.fingerprint],
            )?;
            stats.reread += 1;
        }
        for path in known.keys().filter(|p| !seen.contains(*p)) {
            tx.execute("DELETE FROM records WHERE file = ?1", [path])?;
            tx.execute("DELETE FROM files WHERE path = ?1", [path])?;
            stats.removed += 1;
        }
        tx.commit()?;
        Ok(stats)
    }

    /// Registros desde `since` (segundos Unix, incluido), contando una sola vez las respuestas
    /// repetidas entre archivos. El costo queda `Real` (si la fuente lo trajo) o `Unknown` (falta
    /// aplicar la tabla de precios). La clave de cada respuesta solo sirve para no repetirla: no se
    /// devuelve (`key` = `None`), así no ocupa memoria mientras la vista está abierta.
    pub fn records_since(&self, since: i64) -> rusqlite::Result<Vec<UsageRecord>> {
        let mut stmt = self.con.prepare(
            "SELECT agent, model, ts, input, output, cache_read, cache_write, session_id, title, project,
                    real_cost, key
             FROM records WHERE ts >= ?1 ORDER BY file, id",
        )?;
        let rows = stmt.query_map([since], |r| {
            let count = |i: usize| -> rusqlite::Result<u64> { Ok(r.get::<_, i64>(i)?.max(0) as u64) };
            Ok(UsageRecord {
                agent: agent_from_code(r.get(0)?),
                model: r.get(1)?,
                ts: r.get(2)?,
                input: count(3)?,
                output: count(4)?,
                cache_read: count(5)?,
                cache_write: count(6)?,
                session_id: r.get(7)?,
                title: r.get(8)?,
                project: r.get(9)?,
                cost: r
                    .get::<_, Option<i64>>(10)?
                    .map_or(Cost::Unknown, |m| Cost::Real(m.max(0) as u64)),
                key: r.get(11)?,
            })
        })?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for record in rows {
            let mut record = record?;
            if let Some(key) = record.key.take()
                && !seen.insert((record.agent, key))
            {
                continue;
            }
            out.push(record);
        }
        out.shrink_to_fit();
        Ok(out)
    }
}

/// Archivos de origen con su huella. La de cada rollout de Codex incluye la del índice de títulos
/// (si cambia un título, se releen); la de OpenCode incluye su `-wal` (SQLite escribe ahí primero).
fn units(s: &Sources) -> Vec<Unit> {
    let mut out = Vec::new();
    for path in claude::files(&s.claude_root) {
        if let Some(fingerprint) = fingerprint(&path) {
            out.push(Unit {
                path,
                fingerprint,
                kind: Kind::Claude,
            });
        }
    }
    let index = codex::titles_path(&s.codex_root)
        .and_then(|p| fingerprint(&p))
        .unwrap_or_default();
    for path in codex::files(&s.codex_root) {
        if let Some(fp) = fingerprint(&path) {
            out.push(Unit {
                path,
                fingerprint: format!("{fp}|{index}"),
                kind: Kind::Codex,
            });
        }
    }
    if let Some(fp) = fingerprint(&s.opencode_db) {
        let mut wal = s.opencode_db.clone().into_os_string();
        wal.push("-wal");
        let wal = fingerprint(Path::new(&wal)).unwrap_or_default();
        out.push(Unit {
            path: s.opencode_db.clone(),
            fingerprint: format!("{fp}|{wal}"),
            kind: Kind::OpenCode,
        });
    }
    out
}

/// "tamaño:fecha de modificación en ns", o `None` si no se puede leer.
fn fingerprint(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Some(format!("{}:{modified}", meta.len()))
}

fn agent_code(kind: AgentKind) -> i64 {
    match kind {
        AgentKind::ClaudeCode => 1,
        AgentKind::Codex => 2,
        AgentKind::OpenCode => 3,
        AgentKind::Qwen => 4,
        AgentKind::Gemini => 5,
        AgentKind::Other => 0,
    }
}

fn agent_from_code(code: i64) -> AgentKind {
    match code {
        1 => AgentKind::ClaudeCode,
        2 => AgentKind::Codex,
        3 => AgentKind::OpenCode,
        4 => AgentKind::Qwen,
        5 => AgentKind::Gemini,
        _ => AgentKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, req: &str, session: &str, at: &str, input: u64) -> String {
        format!(
            concat!(
                r#"{{"type":"assistant","requestId":"{req}","sessionId":"{session}","timestamp":"{at}","#,
                r#""cwd":"C:\\proyectos\\demo","message":{{"id":"{id}","model":"claude-demo","#,
                r#""usage":{{"input_tokens":{input},"output_tokens":2,"#,
                r#""cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
                "\n"
            ),
            id = id,
            req = req,
            session = session,
            at = at,
            input = input
        )
    }

    /// Carpeta temporal propia de cada prueba, con las tres fuentes dentro.
    fn sources(name: &str) -> Sources {
        let home = std::env::temp_dir().join(format!("kuntur-usage-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let s = Sources::under_home(&home);
        std::fs::create_dir_all(s.claude_root.join("proyecto")).unwrap();
        s
    }

    #[test]
    fn importar_dos_veces_no_relee_ni_duplica() {
        let s = sources("dos-veces");
        let file = s.claude_root.join("proyecto").join("a.jsonl");
        std::fs::write(&file, line("m1", "r1", "s1", "2026-11-20T10:00:00Z", 10)).unwrap();
        let mut store = UsageStore::open_in_memory().unwrap();
        assert_eq!(
            store.import(&s).unwrap(),
            ImportStats {
                scanned: 1,
                reread: 1,
                removed: 0
            }
        );
        assert_eq!(store.import(&s).unwrap().reread, 0);
        let records = store.records_since(i64::MIN).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].input, 10);
        // La clave solo sirve para no repetir respuestas: no se devuelve.
        assert_eq!(records[0].key, None);
    }

    #[test]
    fn solo_devuelve_desde_el_inicio_de_la_ventana() {
        let s = sources("ventana");
        let file = s.claude_root.join("proyecto").join("s1.jsonl");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let text = [
            line("m1", "r1", "s1", "2026-10-01T10:00:00Z", 1),
            line("m2", "r2", "s1", "2026-11-20T10:00:00Z", 2),
        ]
        .concat();
        std::fs::write(&file, text).unwrap();
        let mut store = UsageStore::open_in_memory().unwrap();
        store.import(&s).unwrap();
        let since = crate::parse_iso("2026-11-01T00:00:00Z").unwrap();
        let records = store.records_since(since).unwrap();
        assert_eq!(records.iter().map(|r| r.input).collect::<Vec<_>>(), [2]);
        assert_eq!(store.records_since(i64::MIN).unwrap().len(), 2);
    }

    #[test]
    fn solo_relee_lo_que_cambio_y_borra_lo_que_desaparece() {
        let s = sources("cambios");
        let a = s.claude_root.join("proyecto").join("a.jsonl");
        let b = s.claude_root.join("proyecto").join("b.jsonl");
        std::fs::write(&a, line("m1", "r1", "s1", "2026-11-20T10:00:00Z", 10)).unwrap();
        std::fs::write(&b, line("m2", "r2", "s2", "2026-11-20T11:00:00Z", 20)).unwrap();
        let mut store = UsageStore::open_in_memory().unwrap();
        assert_eq!(store.import(&s).unwrap().reread, 2);

        // Se agregan líneas a `a`: solo ese archivo se relee.
        let mut text = std::fs::read_to_string(&a).unwrap();
        text.push_str(&line("m3", "r3", "s1", "2026-11-20T10:05:00Z", 5));
        std::fs::write(&a, text).unwrap();
        assert_eq!(
            store.import(&s).unwrap(),
            ImportStats {
                scanned: 2,
                reread: 1,
                removed: 0
            }
        );
        assert_eq!(store.records_since(i64::MIN).unwrap().len(), 3);

        // `b` desaparece: sus registros se van.
        std::fs::remove_file(&b).unwrap();
        assert_eq!(store.import(&s).unwrap().removed, 1);
        let total: u64 = store.records_since(i64::MIN).unwrap().iter().map(|r| r.input).sum();
        assert_eq!(total, 15);
    }

    #[test]
    fn una_respuesta_repetida_en_otro_archivo_se_cuenta_una_vez() {
        let s = sources("repetida");
        let original = line("m1", "r1", "s1", "2026-11-20T10:00:00Z", 10);
        std::fs::write(s.claude_root.join("proyecto").join("a.jsonl"), &original).unwrap();
        // Sesión reanudada: copia la respuesta anterior y agrega una nueva.
        let reanudada = format!("{original}{}", line("m2", "r2", "s2", "2026-11-21T10:00:00Z", 7));
        std::fs::write(s.claude_root.join("proyecto").join("b.jsonl"), reanudada).unwrap();
        let mut store = UsageStore::open_in_memory().unwrap();
        store.import(&s).unwrap();
        let records = store.records_since(i64::MIN).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records.iter().map(|r| r.input).sum::<u64>(), 17);
    }

    #[test]
    fn se_conserva_en_disco_y_sin_fuentes_queda_vacio() {
        let s = sources("disco");
        std::fs::write(
            s.claude_root.join("proyecto").join("a.jsonl"),
            line("m1", "r1", "s1", "2026-11-20T10:00:00Z", 10),
        )
        .unwrap();
        let db = s.claude_root.parent().unwrap().join("usage-prueba.db");
        {
            let mut store = UsageStore::open(&db).unwrap();
            store.import(&s).unwrap();
        }
        let mut store = UsageStore::open(&db).unwrap();
        assert_eq!(store.records_since(i64::MIN).unwrap().len(), 1);
        assert_eq!(store.import(&s).unwrap().reread, 0, "la huella quedó guardada");

        let vacio = Sources::under_home(&std::env::temp_dir().join("kuntur-no-existe-store"));
        let mut otro = UsageStore::open_in_memory().unwrap();
        assert_eq!(otro.import(&vacio).unwrap(), ImportStats::default());
        assert!(otro.records_since(i64::MIN).unwrap().is_empty());
    }

    #[test]
    fn el_costo_real_se_guarda_y_el_estimado_no() {
        let store = UsageStore::open_in_memory().unwrap();
        store
            .con
            .execute(
                "INSERT INTO records (file, agent, model, ts, input, output, cache_read, cache_write,
                                      session_id, real_cost)
                 VALUES ('x', 3, 'm', 1, 1, 1, 0, 0, 's', 1234)",
                [],
            )
            .unwrap();
        let records = store.records_since(i64::MIN).unwrap();
        assert_eq!(records[0].agent, AgentKind::OpenCode);
        assert_eq!(records[0].cost, Cost::Real(1234));
    }
}

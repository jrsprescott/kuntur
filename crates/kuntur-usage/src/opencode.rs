//! Lector de OpenCode: `~/.local/share/opencode/opencode.db` (SQLite), **solo lectura**.
//!
//! Solo se consultan las tablas `session` y `session_v2` (nunca `credential`, `account` ni
//! `control_account`). Columnas usadas (2026-11): `id, title, directory, model, tokens_input,
//! tokens_output, tokens_cache_read, tokens_cache_write, time_created, time_updated, cost`.
//! - `cost` (USD, REAL) es el costo que OpenCode registró: se usa como costo **real**.
//! - `model` es JSON `{"id", "providerID", "variant"?}` (o `NULL`); se acepta también texto plano.
//! - `time_created` / `time_updated` vienen en milisegundos: se normalizan a segundos.
//! - La mayoría de los `id` están en ambas tablas: se cuenta cada sesión una vez (la fila con
//!   `time_updated` más reciente; a igualdad, la de `session_v2`).
//!
//! OpenCode solo guarda totales por sesión: un registro por sesión con la fecha de creación.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::model::{AgentKind, Cost, UsageRecord};

/// Tablas que se leen (y las únicas que se consultan, incluido `PRAGMA table_info`).
const TABLES: [&str; 2] = ["session", "session_v2"];
const COLUMNS: [&str; 11] = [
    "id",
    "title",
    "directory",
    "model",
    "tokens_input",
    "tokens_output",
    "tokens_cache_read",
    "tokens_cache_write",
    "time_created",
    "time_updated",
    "cost",
];

/// Lee las sesiones de la base de OpenCode. Base inexistente o ilegible → vacío.
pub fn read_db(path: &Path) -> Vec<UsageRecord> {
    if !path.is_file() {
        return Vec::new();
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(con) = Connection::open_with_flags(path, flags) else {
        return Vec::new();
    };

    // Por id: (time_updated, registro). Se recorre session_v2 al final para que gane a igualdad.
    let mut by_id: HashMap<String, (i64, UsageRecord)> = HashMap::new();
    for table in TABLES {
        for (updated, record) in read_table(&con, table).unwrap_or_default() {
            match by_id.get(&record.session_id) {
                Some((prev, _)) if *prev > updated => {}
                _ => {
                    by_id.insert(record.session_id.clone(), (updated, record));
                }
            }
        }
    }
    by_id.into_values().map(|(_, r)| r).collect()
}

/// Filas de una tabla como (time_updated en segundos, registro). Columnas ausentes → `NULL`.
fn read_table(con: &Connection, table: &str) -> rusqlite::Result<Vec<(i64, UsageRecord)>> {
    let present: Vec<String> = con
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    if !present.iter().any(|c| c == "id") {
        return Ok(Vec::new());
    }
    let select: Vec<String> = COLUMNS
        .iter()
        .map(|c| {
            if present.iter().any(|p| p == c) {
                (*c).to_string()
            } else {
                format!("NULL AS {c}")
            }
        })
        .collect();
    let sql = format!("SELECT {} FROM {table}", select.join(", "));
    let mut stmt = con.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let count =
            |i: usize| -> rusqlite::Result<u64> { Ok(row.get::<_, Option<i64>>(i)?.unwrap_or(0).max(0) as u64) };
        let created = to_seconds(row.get::<_, Option<i64>>(8)?.unwrap_or(0));
        let updated = row.get::<_, Option<i64>>(9)?.map(to_seconds).unwrap_or(created);
        let record = UsageRecord {
            agent: AgentKind::OpenCode,
            model: model_name(row.get::<_, Option<String>>(3)?.as_deref()),
            ts: created,
            input: count(4)?,
            output: count(5)?,
            cache_read: count(6)?,
            cache_write: count(7)?,
            session_id: row.get(0)?,
            title: row.get(1)?,
            project: row.get(2)?,
            cost: real_cost(row.get::<_, Option<f64>>(10)?),
            key: None,
        };
        Ok((updated, record))
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Costo real en micro-dólares si OpenCode lo registró (> 0).
fn real_cost(usd: Option<f64>) -> Cost {
    match usd {
        Some(v) if v.is_finite() && v > 0.0 => Cost::Real((v * 1e6).round() as u64),
        _ => Cost::Unknown,
    }
}

/// OpenCode guarda milisegundos; si el valor parece estar en segundos se deja igual.
fn to_seconds(t: i64) -> i64 {
    if t.abs() >= 100_000_000_000 { t / 1000 } else { t }
}

/// Nombre del modelo desde `{"id", "providerID"}` o texto plano. Si el nombre solo no deja ver el
/// proveedor (p. ej. un alias), se antepone `providerID/`.
fn model_name(raw: Option<&str>) -> String {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    let field = |k: &str| value.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (id, provider) = (field("id"), field("providerID"));
    if id.is_empty() {
        return if value.is_string() {
            value.as_str().unwrap_or_default().to_string()
        } else {
            String::new()
        };
    }
    let qualified = format!("{provider}/{id}");
    // Un modelo local conserva su proveedor (si no, `qwen…` en LM Studio parecería de la nube).
    if crate::pricing::is_local_model(&qualified) {
        return qualified;
    }
    if crate::provider_of(&id) == "Otros" && !provider.is_empty() && crate::provider_of(&qualified) != "Otros" {
        qualified
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "id TEXT PRIMARY KEY, title TEXT, directory TEXT, model TEXT, tokens_input INTEGER, \
        tokens_output INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, \
        time_created INTEGER, time_updated INTEGER";

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-usage-opencode-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("opencode.db")
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        con: &Connection,
        table: &str,
        id: &str,
        title: &str,
        model: Option<&str>,
        tokens: [i64; 4],
        created: i64,
        updated: i64,
    ) {
        con.execute(
            &format!(
                "INSERT INTO {table} (id, title, directory, model, tokens_input, tokens_output, tokens_cache_read, \
                 tokens_cache_write, time_created, time_updated) VALUES (?1, ?2, 'C:\\src\\demo', ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
            ),
            rusqlite::params![id, title, model, tokens[0], tokens[1], tokens[2], tokens[3], created, updated],
        )
        .unwrap();
    }

    #[test]
    fn modelos_locales_conservan_su_proveedor() {
        // Un modelo con nombre de marca servido por LM Studio u Ollama no debe confundirse con la nube.
        assert_eq!(
            model_name(Some(r#"{"id":"qwen2.5-coder-7b","providerID":"lmstudio"}"#)),
            "lmstudio/qwen2.5-coder-7b"
        );
        assert_eq!(
            model_name(Some(r#"{"id":"llama3.1:8b","providerID":"ollama"}"#)),
            "ollama/llama3.1:8b"
        );
        // En la nube sigue como antes: solo el nombre.
        assert_eq!(
            model_name(Some(r#"{"id":"claude-test-4","providerID":"anthropic"}"#)),
            "claude-test-4"
        );
    }

    #[test]
    fn una_sesion_por_registro() {
        let path = tmp("dos");
        {
            let con = Connection::open(&path).unwrap();
            con.execute(&format!("CREATE TABLE session ({SCHEMA})"), []).unwrap();
            let json = r#"{"id":"claude-test-4","providerID":"anthropic"}"#;
            insert(
                &con,
                "session",
                "a",
                "Primera",
                Some(json),
                [10, 20, 30, 40],
                1_763_000_000_000,
                1_763_000_100_000,
            );
            insert(
                &con,
                "session",
                "b",
                "Segunda",
                Some("gpt-test"),
                [1, 2, 0, 0],
                1_763_000_500,
                1_763_000_600,
            );
        }
        let mut records = read_db(&path);
        records.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        assert_eq!(records.len(), 2);
        let a = &records[0];
        assert_eq!(a.agent, AgentKind::OpenCode);
        assert_eq!(a.model, "claude-test-4");
        assert_eq!((a.input, a.output, a.cache_read, a.cache_write), (10, 20, 30, 40));
        assert_eq!(a.ts, 1_763_000_000);
        assert_eq!(a.title.as_deref(), Some("Primera"));
        assert_eq!(a.project.as_deref(), Some(r"C:\src\demo"));
        assert_eq!(records[1].model, "gpt-test");
        assert_eq!(records[1].ts, 1_763_000_500);
        // Sin columna `cost`: costo desconocido (se estimará con la tabla de precios).
        assert_eq!(a.cost, Cost::Unknown);
    }

    #[test]
    fn el_costo_registrado_por_opencode_es_real() {
        let path = tmp("costo");
        {
            let con = Connection::open(&path).unwrap();
            con.execute(&format!("CREATE TABLE session ({SCHEMA}, cost REAL)"), [])
                .unwrap();
            insert(
                &con,
                "session",
                "a",
                "Con costo",
                None,
                [1, 1, 0, 0],
                1_763_000_000,
                1_763_000_000,
            );
            insert(
                &con,
                "session",
                "b",
                "Gratis",
                None,
                [1, 1, 0, 0],
                1_763_000_000,
                1_763_000_000,
            );
            con.execute("UPDATE session SET cost = 0.0125 WHERE id = 'a'", [])
                .unwrap();
            con.execute("UPDATE session SET cost = 0 WHERE id = 'b'", []).unwrap();
        }
        let mut records = read_db(&path);
        records.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        assert_eq!(records[0].cost, Cost::Real(12_500));
        assert_eq!(records[1].cost, Cost::Unknown);
    }

    #[test]
    fn no_duplica_sesiones_entre_session_y_session_v2() {
        let path = tmp("v2");
        {
            let con = Connection::open(&path).unwrap();
            con.execute(&format!("CREATE TABLE session ({SCHEMA})"), []).unwrap();
            con.execute(&format!("CREATE TABLE session_v2 ({SCHEMA}, time_busy INTEGER)"), [])
                .unwrap();
            // "a" está en ambas: gana la fila actualizada más tarde (la de session_v2).
            insert(
                &con,
                "session",
                "a",
                "Vieja",
                None,
                [1, 1, 0, 0],
                1_763_000_000_000,
                1_763_000_000_000,
            );
            insert(
                &con,
                "session_v2",
                "a",
                "Nueva",
                None,
                [5, 5, 0, 0],
                1_763_000_000_000,
                1_763_000_900_000,
            );
            insert(
                &con,
                "session_v2",
                "c",
                "Solo v2",
                None,
                [2, 2, 0, 0],
                1_763_000_000_000,
                1_763_000_000_000,
            );
        }
        let mut records = read_db(&path);
        records.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].title.as_deref(), Some("Nueva"));
        assert_eq!(records[0].input, 5);
        assert_eq!(records[0].model, "");
        assert_eq!(records[1].session_id, "c");
    }

    #[test]
    fn base_inexistente_da_vacio_sin_crearla() {
        let path = tmp("nada");
        assert!(read_db(&path).is_empty());
        assert!(!path.exists(), "abrir en solo lectura no debe crear la base");
    }
}

//! Lector del historial de Codex: `~/.codex/sessions/AAAA/MM/DD/rollout-*.jsonl.zst` (zstd) o `.jsonl`.
//!
//! Nombres de campos confirmados con archivos reales (solo claves, 2026-11):
//! - Cada línea: `{timestamp (ISO 8601), type, payload, ordinal}`.
//! - `type:"session_meta"` → `payload.{id, session_id, cwd, model_provider, timestamp, …}` (`id` = `session_id`).
//! - `type:"turn_context"` → `payload.{model, cwd, turn_id, effort, …}`.
//! - `type:"event_msg"` con `payload.type:"token_count"` → `payload.info` (puede ser `null`) con
//!   `last_token_usage` (el turno) y `total_token_usage` (acumulado), ambos
//!   `{input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens,
//!   reasoning_output_tokens, total_tokens}`; y `payload.rate_limits.{primary, secondary}` (o `null`),
//!   cada uno `{used_percent (float), window_minutes, resets_at (segundos Unix)}`.
//! - `input_tokens` **incluye** `cached_input_tokens`, y `output_tokens` incluye el razonamiento
//!   (`total_tokens = input_tokens + output_tokens`). Aquí la entrada se guarda sin la caché.
//! - `type:"token_usage_record"` repite el mismo uso por turno: se ignora para no contar dos veces.
//! - Títulos: `~/.codex/session_index.jsonl` con `{id, thread_name, updated_at}` (`id` = sesión).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::model::{AgentKind, UsageRecord};

/// Uso de los límites de Codex (para el anillo del panel). Porcentajes 0–100.
#[derive(Clone, Debug, PartialEq)]
pub struct Limits {
    pub primary_pct: Option<f64>,
    pub primary_window_mins: Option<u64>,
    /// Segundos Unix.
    pub primary_resets_at: Option<i64>,
    pub secondary_pct: Option<f64>,
    pub secondary_window_mins: Option<u64>,
    pub secondary_resets_at: Option<i64>,
    /// Cuándo se informó (segundos Unix).
    pub ts: i64,
}

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    info: Option<Info>,
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Info {
    last_token_usage: Option<TokenUsage>,
    total_token_usage: Option<TokenUsage>,
}

#[derive(Deserialize)]
struct TokenUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    primary: Option<Window>,
    secondary: Option<Window>,
}

#[derive(Deserialize)]
struct Window {
    used_percent: Option<f64>,
    window_minutes: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize)]
struct IndexEntry {
    id: Option<String>,
    thread_name: Option<String>,
}

/// Solo interesan tres clases de línea; el resto (mensajes, herramientas) se salta sin parsear.
fn parse_line(raw: &str) -> Option<Line> {
    if !(raw.contains("token_count") || raw.contains("turn_context") || raw.contains("session_meta")) {
        return None;
    }
    serde_json::from_str(raw).ok()
}

fn is_token_count(payload: &Payload) -> bool {
    payload.kind.as_deref() == Some("token_count")
}

/// Texto de un rollout, descomprimiendo `.zst`. `None` si no se puede leer.
fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if path.extension().is_some_and(|e| e == "zst") {
        let raw = zstd::decode_all(bytes.as_slice()).ok()?;
        Some(String::from_utf8_lossy(&raw).into_owned())
    } else {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

fn rollouts(root: &Path) -> Vec<std::path::PathBuf> {
    crate::files_under(root, &[".jsonl", ".jsonl.zst"])
}

/// Nombre del archivo sin `.jsonl` / `.jsonl.zst`.
fn file_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.trim_end_matches(".zst").trim_end_matches(".jsonl").to_string()
}

/// Títulos por sesión de `session_index.jsonl` (el último gana).
fn read_titles(path: &Path) -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<IndexEntry>(l).ok())
        .filter_map(|e| Some((e.id?, e.thread_name?)))
        .collect()
}

/// Lee los rollouts bajo `root` (`.jsonl` y `.jsonl.zst`) y, si existe, los títulos de
/// `session_index.jsonl` en la carpeta superior.
pub fn read_dir(root: &Path) -> Vec<UsageRecord> {
    let titles = root
        .parent()
        .map(|p| read_titles(&p.join("session_index.jsonl")))
        .unwrap_or_default();
    let mut records = Vec::new();
    for path in rollouts(root) {
        let Some(text) = read_text(&path) else { continue };
        records.extend(parse_lines(&text, &file_stem(&path)));
    }
    for r in &mut records {
        r.title = titles.get(&r.session_id).cloned();
    }
    records
}

/// Lee un rollout ya descomprimido. `fallback_session` identifica la sesión si falta `session_meta`.
pub fn parse_lines(text: &str, fallback_session: &str) -> Vec<UsageRecord> {
    let mut session = fallback_session.to_string();
    let mut meta_cwd: Option<String> = None;
    let mut turn_cwd: Option<String> = None;
    let mut model = String::new();
    let mut previous_total: Option<u64> = None;
    let mut records = Vec::new();

    for line in text.lines().filter_map(parse_line) {
        let Some(payload) = line.payload else { continue };
        match line.kind.as_deref() {
            Some("session_meta") => {
                if let Some(id) = payload.id.or(payload.session_id) {
                    session = id;
                }
                meta_cwd = payload.cwd.or(meta_cwd);
            }
            Some("turn_context") => {
                model = payload.model.unwrap_or(model);
                turn_cwd = payload.cwd.or(turn_cwd);
            }
            Some("event_msg") if is_token_count(&payload) => {
                let Some(info) = payload.info else { continue };
                let Some(last) = info.last_token_usage else { continue };
                // Codex a veces repite el evento sin un turno nuevo: el acumulado no cambia.
                let total = info.total_token_usage.map(|t| t.total_tokens);
                if total.is_some() && total == previous_total {
                    continue;
                }
                previous_total = total;
                let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
                    continue;
                };
                records.push(UsageRecord {
                    agent: AgentKind::Codex,
                    model: model.clone(),
                    ts,
                    input: last.input_tokens.saturating_sub(last.cached_input_tokens),
                    output: last.output_tokens,
                    cache_read: last.cached_input_tokens,
                    cache_write: last.cache_write_input_tokens,
                    session_id: session.clone(),
                    title: None,
                    project: turn_cwd.clone().or_else(|| meta_cwd.clone()),
                });
            }
            _ => {}
        }
    }
    records
}

/// Últimos límites informados en un rollout.
fn last_limits(text: &str) -> Option<Limits> {
    let mut found = None;
    for line in text.lines().filter_map(parse_line) {
        let Some(payload) = line.payload else { continue };
        if line.kind.as_deref() != Some("event_msg") || !is_token_count(&payload) {
            continue;
        }
        let Some(limits) = payload.rate_limits else { continue };
        if limits.primary.is_none() && limits.secondary.is_none() {
            continue;
        }
        let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
            continue;
        };
        let (p, s) = (limits.primary.as_ref(), limits.secondary.as_ref());
        found = Some(Limits {
            primary_pct: p.and_then(|w| w.used_percent),
            primary_window_mins: p.and_then(|w| w.window_minutes),
            primary_resets_at: p.and_then(|w| w.resets_at),
            secondary_pct: s.and_then(|w| w.used_percent),
            secondary_window_mins: s.and_then(|w| w.window_minutes),
            secondary_resets_at: s.and_then(|w| w.resets_at),
            ts,
        });
    }
    found
}

/// Límites del evento `token_count` más reciente con datos bajo `root`. Recorre los archivos del más
/// nuevo al más viejo (por fecha de modificación) y para cuando ninguno puede tener algo más reciente.
pub fn latest_limits(root: &Path) -> Option<Limits> {
    let mtime = |p: &Path| -> i64 {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(i64::MAX)
    };
    let mut files: Vec<_> = rollouts(root).into_iter().map(|p| (mtime(&p), p)).collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));

    let mut best: Option<Limits> = None;
    for (modified, path) in files {
        if best.as_ref().is_some_and(|b| modified < b.ts) {
            break;
        }
        let Some(limits) = read_text(&path).as_deref().and_then(last_limits) else {
            continue;
        };
        if best.as_ref().is_none_or(|b| limits.ts > b.ts) {
            best = Some(limits);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, cached: u64, output: u64, total: u64) -> String {
        format!(
            r#"{{"input_tokens":{input},"cached_input_tokens":{cached},"cache_write_input_tokens":0,"output_tokens":{output},"reasoning_output_tokens":0,"total_tokens":{total}}}"#
        )
    }

    fn token_count(at: &str, last: &str, total: &str, primary: &str) -> String {
        format!(
            r#"{{"timestamp":"{at}","type":"event_msg","ordinal":1,"payload":{{"type":"token_count","info":{{"last_token_usage":{last},"total_token_usage":{total},"model_context_window":1000}},"rate_limits":{{"limit_id":"x","primary":{primary},"secondary":null}}}}}}"#
        )
    }

    fn rollout() -> String {
        let pct = |p: f64| format!(r#"{{"used_percent":{p},"window_minutes":300,"resets_at":1800000000}}"#);
        [
            r#"{"timestamp":"2026-11-20T10:00:00.000Z","type":"session_meta","payload":{"id":"s1","session_id":"s1","cwd":"C:\\src\\meta","model_provider":"openai"}}"#.to_string(),
            r#"{"timestamp":"2026-11-20T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-test","cwd":"C:\\src\\turno"}}"#.to_string(),
            r#"{"timestamp":"2026-11-20T10:00:02.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":null,"secondary":null}}}"#.to_string(),
            token_count("2026-11-20T10:00:05.000Z", &usage(100, 40, 10, 110), &usage(100, 40, 10, 110), &pct(10.0)),
            // Evento repetido con el mismo acumulado: no es un turno nuevo.
            token_count("2026-11-20T10:00:06.000Z", &usage(100, 40, 10, 110), &usage(100, 40, 10, 110), &pct(10.0)),
            r#"{"timestamp":"2026-11-20T10:00:07.000Z","type":"token_usage_record","payload":{"usage":{"input_tokens":100}}}"#.to_string(),
            r#"{"timestamp":"#.to_string(),
            r#"{"timestamp":"2026-11-20T10:01:00.000Z","type":"turn_context","payload":{"model":"gpt-otro","cwd":"C:\\src\\turno"}}"#.to_string(),
            token_count("2026-11-20T10:01:05.000Z", &usage(50, 0, 5, 55), &usage(150, 40, 15, 165), &pct(12.5)),
        ]
        .join("\n")
    }

    #[test]
    fn un_registro_por_turno_con_la_entrada_sin_cache() {
        let records = parse_lines(&rollout(), "archivo");
        assert_eq!(records.len(), 2);
        let r = &records[0];
        assert_eq!(r.agent, AgentKind::Codex);
        assert_eq!(r.model, "gpt-test");
        assert_eq!((r.input, r.cache_read, r.output, r.cache_write), (60, 40, 10, 0));
        assert_eq!(r.session_id, "s1");
        assert_eq!(r.project.as_deref(), Some(r"C:\src\turno"));
        assert_eq!(r.ts, crate::parse_iso("2026-11-20T10:00:05Z").unwrap());
        assert_eq!(records[1].model, "gpt-otro");
        assert_eq!((records[1].input, records[1].output), (50, 5));
    }

    #[test]
    fn sin_session_meta_usa_el_nombre_del_archivo() {
        let text = token_count("2026-11-20T10:00:05Z", &usage(1, 0, 1, 2), &usage(1, 0, 1, 2), "null");
        let records = parse_lines(&text, "rollout-x");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "rollout-x");
        assert_eq!(records[0].project, None);
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-usage-codex-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn lee_rollouts_comprimidos_y_titulos() {
        let base = tmp("leer");
        let day = base.join("sessions").join("2026").join("11").join("20");
        std::fs::create_dir_all(&day).unwrap();
        let comprimido = zstd::encode_all(rollout().as_bytes(), 0).unwrap();
        std::fs::write(day.join("rollout-a.jsonl.zst"), comprimido).unwrap();
        let plano = token_count("2026-11-21T10:00:05Z", &usage(7, 0, 3, 10), &usage(7, 0, 3, 10), "null");
        std::fs::write(day.join("rollout-b.jsonl"), plano).unwrap();
        std::fs::write(day.join("rollout-c.jsonl.zst"), b"no es zstd").unwrap();
        std::fs::write(
            base.join("session_index.jsonl"),
            "{\"id\":\"s1\",\"thread_name\":\"Viejo\",\"updated_at\":\"x\"}\n{\"id\":\"s1\",\"thread_name\":\"Nuevo\",\"updated_at\":\"y\"}\n",
        )
        .unwrap();

        let records = read_dir(&base.join("sessions"));
        let limits = latest_limits(&base.join("sessions"));
        let _ = std::fs::remove_dir_all(&base);

        assert_eq!(records.len(), 3);
        let s1: Vec<_> = records.iter().filter(|r| r.session_id == "s1").collect();
        assert_eq!(s1.len(), 2);
        assert!(s1.iter().all(|r| r.title.as_deref() == Some("Nuevo")));
        let otro = records.iter().find(|r| r.session_id != "s1").unwrap();
        assert_eq!(otro.session_id, "rollout-b");
        assert_eq!(otro.title, None);

        let limits = limits.expect("hay límites");
        assert_eq!(limits.primary_pct, Some(12.5));
        assert_eq!(limits.primary_window_mins, Some(300));
        assert_eq!(limits.primary_resets_at, Some(1_800_000_000));
        assert_eq!(limits.secondary_pct, None);
        assert_eq!(limits.ts, crate::parse_iso("2026-11-20T10:01:05Z").unwrap());
    }

    #[test]
    fn los_limites_mas_recientes_ganan_entre_archivos() {
        let base = tmp("limites");
        std::fs::create_dir_all(&base).unwrap();
        let lim = |p: f64, s: f64| {
            format!(
                r#"{{"used_percent":{p},"window_minutes":300,"resets_at":1}},"secondary":{{"used_percent":{s},"window_minutes":10080,"resets_at":2}}"#
            )
        };
        let evento = |at: &str, p: f64, s: f64| {
            format!(
                r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{{"primary":{}}}}}}}"#,
                lim(p, s)
            )
        };
        // El archivo "b" tiene el evento más nuevo aunque su nombre ordene después de "a".
        std::fs::write(base.join("a.jsonl"), evento("2026-11-20T09:00:00Z", 30.0, 60.0)).unwrap();
        std::fs::write(base.join("b.jsonl"), evento("2026-11-20T11:00:00Z", 44.0, 12.0)).unwrap();
        let limits = latest_limits(&base);
        let vacio = latest_limits(&base.join("no-existe"));
        let _ = std::fs::remove_dir_all(&base);

        let limits = limits.unwrap();
        assert_eq!((limits.primary_pct, limits.secondary_pct), (Some(44.0), Some(12.0)));
        assert_eq!(limits.secondary_window_mins, Some(10080));
        assert_eq!(limits.secondary_resets_at, Some(2));
        assert_eq!(vacio, None);
        assert!(read_dir(&std::env::temp_dir().join("kuntur-no-existe-codex")).is_empty());
    }
}

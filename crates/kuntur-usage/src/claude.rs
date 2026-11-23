//! Lector del historial de Claude Code: `~/.claude/projects/<carpeta>/*.jsonl` (y subcarpetas).
//!
//! Campos usados (2026-11):
//! - Respuestas: `type:"assistant"`, `message.{id, model, usage{input_tokens, output_tokens,
//!   cache_creation_input_tokens, cache_read_input_tokens}}`, `requestId`, `timestamp` (ISO 8601),
//!   `cwd`, `sessionId`. Una misma respuesta se repite en varias líneas (una por bloque de contenido):
//!   se cuenta una sola vez por (`message.id`, `requestId`).
//! - Títulos: `type:"ai-title"` (`aiTitle`) y `type:"custom-title"` (`customTitle`), con `sessionId`.
//!   El título que pone el usuario (`custom-title`) gana sobre el generado.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

use crate::model::{AgentKind, UsageRecord};

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
    cwd: Option<String>,
    message: Option<Message>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
    #[serde(rename = "customTitle")]
    custom_title: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

/// Estado compartido entre archivos: respuestas ya contadas y títulos por sesión.
#[derive(Default)]
struct Reader {
    seen: HashSet<(String, String)>,
    records: Vec<UsageRecord>,
    ai_titles: HashMap<String, String>,
    custom_titles: HashMap<String, String>,
}

impl Reader {
    fn feed(&mut self, text: &str, project_hint: Option<&str>) {
        for raw in text.lines() {
            // Atajo: la mayoría de las líneas son mensajes del usuario o herramientas.
            if !(raw.contains("\"assistant\"") || raw.contains("-title\"")) {
                continue;
            }
            let Ok(line) = serde_json::from_str::<Line>(raw) else {
                continue;
            };
            match line.kind.as_deref() {
                Some("assistant") => self.assistant(line, project_hint),
                Some("ai-title") => {
                    if let (Some(s), Some(t)) = (line.session_id, line.ai_title) {
                        self.ai_titles.insert(s, t);
                    }
                }
                Some("custom-title") => {
                    if let (Some(s), Some(t)) = (line.session_id, line.custom_title) {
                        self.custom_titles.insert(s, t);
                    }
                }
                _ => {}
            }
        }
    }

    fn assistant(&mut self, line: Line, project_hint: Option<&str>) {
        let Some(message) = line.message else { return };
        let Some(usage) = message.usage else { return };
        let model = message.model.unwrap_or_default();
        // Respuestas que genera el propio cliente (errores, cortes): sin tokens reales.
        if model == "<synthetic>" {
            return;
        }
        let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
            return;
        };
        let key = (message.id.unwrap_or_default(), line.request_id.unwrap_or_default());
        if key != (String::new(), String::new()) && !self.seen.insert(key) {
            return;
        }
        self.records.push(UsageRecord {
            agent: AgentKind::ClaudeCode,
            model,
            ts,
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_input_tokens,
            cache_write: usage.cache_creation_input_tokens,
            session_id: line.session_id.unwrap_or_default(),
            title: None,
            project: line.cwd.or_else(|| project_hint.map(str::to_string)),
        });
    }

    fn finish(mut self) -> Vec<UsageRecord> {
        for r in &mut self.records {
            r.title = self
                .custom_titles
                .get(&r.session_id)
                .or_else(|| self.ai_titles.get(&r.session_id))
                .cloned();
        }
        self.records
    }
}

/// Lee todos los `*.jsonl` bajo `root` (deduplicando respuestas entre archivos). La carpeta de primer
/// nivel (el proyecto codificado por Claude Code) es la pista de carpeta si una línea no trae `cwd`.
pub fn read_dir(root: &Path) -> Vec<UsageRecord> {
    let mut reader = Reader::default();
    for path in crate::files_under(root, &[".jsonl"]) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let hint = path
            .strip_prefix(root)
            .ok()
            .and_then(Path::parent)
            .and_then(|dir| dir.components().next())
            .map(|c| c.as_os_str().to_string_lossy().into_owned());
        reader.feed(&text, hint.as_deref());
    }
    reader.finish()
}

/// Lee el texto de un JSONL. `project_hint` se usa como carpeta si la línea no trae `cwd`.
pub fn parse_lines(text: &str, project_hint: Option<&str>) -> Vec<UsageRecord> {
    let mut reader = Reader::default();
    reader.feed(text, project_hint);
    reader.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(id: &str, req: &str, session: &str, input: u64, output: u64) -> String {
        format!(
            concat!(
                r#"{{"type":"assistant","requestId":"{req}","sessionId":"{session}","#,
                r#""timestamp":"2026-11-20T10:00:00.000Z","cwd":"C:\\src\\demo","#,
                r#""message":{{"id":"{id}","model":"claude-test-1","role":"assistant","#,
                r#""content":[{{"type":"text","text":"hola"}}],"#,
                r#""usage":{{"input_tokens":{input},"output_tokens":{output},"#,
                r#""cache_creation_input_tokens":7,"cache_read_input_tokens":11}}}}}}"#
            ),
            req = req,
            session = session,
            id = id,
            input = input,
            output = output,
        )
    }

    fn sample() -> String {
        [
            r#"{"type":"user","sessionId":"s1","message":{"role":"user","content":"hola"}}"#.to_string(),
            assistant("msg_1", "req_1", "s1", 3, 40),
            // La misma respuesta repetida (otro bloque de contenido): no se cuenta dos veces.
            assistant("msg_1", "req_1", "s1", 3, 40),
            r#"{"type":"assistant","message":{"id":"#.to_string(),
            assistant("msg_2", "req_2", "s1", 5, 60),
            r#"{"type":"ai-title","sessionId":"s1","aiTitle":"Titulo generado"}"#.to_string(),
        ]
        .join("\n")
    }

    #[test]
    fn cuenta_cada_respuesta_una_vez_y_asigna_el_titulo() {
        let records = parse_lines(&sample(), Some("pista"));
        assert_eq!(records.len(), 2);
        let input: u64 = records.iter().map(|r| r.input).sum();
        let output: u64 = records.iter().map(|r| r.output).sum();
        assert_eq!((input, output), (8, 100));
        let r = &records[0];
        assert_eq!(r.agent, AgentKind::ClaudeCode);
        assert_eq!(r.model, "claude-test-1");
        assert_eq!(r.cache_write, 7);
        assert_eq!(r.cache_read, 11);
        assert_eq!(r.session_id, "s1");
        assert_eq!(
            r.ts,
            "2026-11-20T10:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second()
        );
        assert_eq!(r.project.as_deref(), Some(r"C:\src\demo"));
        assert!(records.iter().all(|r| r.title.as_deref() == Some("Titulo generado")));
    }

    #[test]
    fn el_titulo_del_usuario_gana_y_la_pista_cubre_la_carpeta() {
        let text = [
            r#"{"type":"custom-title","sessionId":"s2","customTitle":"Mi titulo"}"#.to_string(),
            r#"{"type":"assistant","requestId":"r","sessionId":"s2","timestamp":"2026-11-20T10:00:00Z","message":{"id":"m","model":"claude-x","usage":{"input_tokens":1,"output_tokens":2}}}"#.to_string(),
            r#"{"type":"ai-title","sessionId":"s2","aiTitle":"Generado"}"#.to_string(),
        ]
        .join("\n");
        let records = parse_lines(&text, Some("pista"));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].title.as_deref(), Some("Mi titulo"));
        assert_eq!(records[0].project.as_deref(), Some("pista"));
        assert_eq!((records[0].cache_read, records[0].cache_write), (0, 0));
    }

    #[test]
    fn lee_una_carpeta_y_deduplica_entre_archivos() {
        let root = std::env::temp_dir().join(format!("kuntur-usage-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("proyecto").join("sub")).unwrap();
        std::fs::write(root.join("proyecto").join("a.jsonl"), sample()).unwrap();
        // Un archivo hijo que repite una respuesta del principal.
        std::fs::write(
            root.join("proyecto").join("sub").join("b.jsonl"),
            assistant("msg_2", "req_2", "s1", 5, 60),
        )
        .unwrap();
        std::fs::write(root.join("proyecto").join("notas.txt"), "no es jsonl").unwrap();
        let records = read_dir(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(records.len(), 2);

        assert!(read_dir(&root).is_empty(), "una carpeta inexistente no falla");
    }
}

//! Comandos de Claude Code: `~/.claude/projects/<carpeta>/*.jsonl` (y subcarpetas, p. ej. subagentes).
//!
//! Campos usados (confirmados con archivos reales, solo claves, 2026-11):
//! - Línea `type:"assistant"` con `sessionId`, `timestamp` (ISO 8601), `cwd` y `message.content[]`.
//! - Bloque `{type:"tool_use", id, name, input}`; `name` = `"Bash"` o `"PowerShell"` →
//!   `input.command` (además trae `description`, `timeout`, `run_in_background`).
//! - Las líneas `type:"api-request-blob"` repiten bloques `tool_use` del historial enviado: se ignoran.
//! - Al reanudar una sesión, Claude Code copia el historial a otro archivo: el `id` del bloque evita
//!   contarlo dos veces (`model::dedup`).

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::model::{AuditRecord, LOCAL_HOST};
use kuntur_usage::AgentKind;

/// Herramientas de Claude Code que ejecutan comandos.
const SHELL_TOOLS: [&str; 2] = ["Bash", "PowerShell"];

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
    cwd: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<Value>,
}

/// Comandos de un archivo ya leído.
pub fn parse_lines(text: &str) -> Vec<AuditRecord> {
    let mut out = Vec::new();
    for raw in text.lines() {
        // Atajo: casi ninguna línea trae una llamada a herramienta.
        if !(raw.contains("\"tool_use\"") && raw.contains("\"assistant\"")) {
            continue;
        }
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            continue;
        };
        if line.kind.as_deref() != Some("assistant") {
            continue;
        }
        let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
            continue;
        };
        let Some(Value::Array(blocks)) = line.message.and_then(|m| m.content) else {
            continue;
        };
        for block in &blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let Some(tool) = block.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !SHELL_TOOLS.contains(&tool) {
                continue;
            }
            let Some(command) = block
                .get("input")
                .and_then(|i| i.get("command"))
                .and_then(Value::as_str)
                .filter(|c| !c.trim().is_empty())
            else {
                continue;
            };
            out.push(AuditRecord {
                ts,
                agent: AgentKind::ClaudeCode,
                session: line.session_id.clone().unwrap_or_default(),
                host: LOCAL_HOST.into(),
                folder: line.cwd.clone(),
                command: command.to_string(),
                tool: tool.to_string(),
                key: block.get("id").and_then(Value::as_str).map(str::to_string),
            });
        }
    }
    out
}

/// Comandos de un archivo (vacío si no se puede leer).
pub fn read_file(path: &Path) -> Vec<AuditRecord> {
    std::fs::read(path)
        .map(|bytes| parse_lines(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

/// Archivos de Claude Code bajo `root`.
pub fn files(root: &Path) -> Vec<std::path::PathBuf> {
    crate::files_under(root, &[".jsonl"])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"type":"user","sessionId":"s1","timestamp":"2026-11-20T10:00:00.000Z","message":{"content":"hola"}}
{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:00:05.000Z","cwd":"C:\\proyectos\\demo","message":{"content":[{"type":"text","text":"Voy a listar"},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls -la","description":"Listar"}}]}}
{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:01:00.000Z","cwd":"C:\\proyectos\\demo","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"PowerShell","input":{"command":"Get-ChildItem","timeout":1000}},{"type":"tool_use","id":"toolu_3","name":"Read","input":{"file_path":"x"}}]}}
{"type":"api-request-blob","sessionId":"s1","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls -la"}}]}}
{"type":"assistant","sessionId":"s1","timestamp":"2026-11-20T10:02:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_4","name":"Bash","input":{"command":"   "}}]}}
{"type":"assistant","sessionId":"s1","timestamp":"no es fecha","message":{"content":[{"type":"tool_use","id":"toolu_5","name":"Bash","input":{"command":"echo"}}]}}
roto {"type":"assistant" "tool_use"
"#;

    #[test]
    fn lee_bash_y_powershell() {
        let r = parse_lines(SAMPLE);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].command, "ls -la");
        assert_eq!(r[0].tool, "Bash");
        assert_eq!(r[0].agent, AgentKind::ClaudeCode);
        assert_eq!(r[0].session, "s1");
        assert_eq!(r[0].host, "local");
        assert_eq!(r[0].folder.as_deref(), Some(r"C:\proyectos\demo"));
        assert_eq!(r[0].key.as_deref(), Some("toolu_1"));
        assert_eq!(r[0].ts, crate::parse_iso("2026-11-20T10:00:05Z").unwrap());
        assert_eq!(r[1].command, "Get-ChildItem");
        assert_eq!(r[1].tool, "PowerShell");
    }

    #[test]
    fn sesion_reanudada_no_repite() {
        let mut both = parse_lines(SAMPLE);
        both.extend(parse_lines(SAMPLE));
        assert_eq!(crate::dedup(both).len(), 2);
    }

    #[test]
    fn lee_archivos_de_una_carpeta() {
        let root = std::env::temp_dir().join(format!("kuntur-audit-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let sub = root.join("proyecto").join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("proyecto").join("s1.jsonl"), SAMPLE).unwrap();
        std::fs::write(sub.join("agent-1.jsonl"), SAMPLE).unwrap();
        std::fs::write(root.join("otro.txt"), SAMPLE).unwrap();
        let found = files(&root);
        let total: usize = found.iter().map(|p| read_file(p).len()).sum();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found.len(), 2);
        assert_eq!(total, 4);
        assert!(read_file(Path::new("no-existe.jsonl")).is_empty());
    }
}

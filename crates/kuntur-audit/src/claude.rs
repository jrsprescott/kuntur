//! Comandos de Claude Code: `~/.claude/projects/<carpeta>/*.jsonl` (y subcarpetas, p. ej. subagentes).
//!
//! Campos usados (confirmados con archivos reales, solo claves, 2026-11):
//! - Línea `type:"assistant"` con `sessionId`, `timestamp` (ISO 8601), `cwd` y `message.content[]`.
//! - Bloque `{type:"tool_use", id, name, input}`; `name` = `"Bash"` o `"PowerShell"` →
//!   `input.command` (además trae `description`, `timeout`, `run_in_background`).
//! - Las líneas `type:"api-request-blob"` repiten bloques `tool_use` del historial enviado: se ignoran.
//! - Al reanudar una sesión, Claude Code copia el historial a otro archivo: el `id` del bloque evita
//!   contarlo dos veces (`model::dedup`).
//! - Títulos del chat: `type:"custom-title"` (`customTitle`, el que pone el usuario) y
//!   `type:"ai-title"` (`aiTitle`, el generado), con `sessionId`. El del usuario gana.
//!
//! Se lee línea por línea y sin armar el JSON completo: los bloques de texto, las salidas de
//! herramientas y los archivos que escribe el agente se saltan sin copiarlos.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use crate::model::{AuditRecord, Interner, LOCAL_HOST};
use kuntur_usage::AgentKind;

/// Herramientas de Claude Code que ejecutan comandos.
const SHELL_TOOLS: [&str; 2] = ["Bash", "PowerShell"];

#[derive(Deserialize)]
struct Line<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(rename = "sessionId", borrow)]
    session_id: Option<Cow<'a, str>>,
    #[serde(borrow)]
    timestamp: Option<Cow<'a, str>>,
    #[serde(borrow)]
    cwd: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Option<Message<'a>>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
    #[serde(rename = "customTitle")]
    custom_title: Option<String>,
}

#[derive(Deserialize)]
struct Message<'a> {
    /// En las líneas del asistente es una lista de bloques; si es texto, la línea no trae comandos
    /// y no se pudo leer (se descarta).
    #[serde(borrow)]
    content: Option<Vec<Block<'a>>>,
}

#[derive(Deserialize)]
struct Block<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    id: Option<Cow<'a, str>>,
    #[serde(borrow)]
    name: Option<Cow<'a, str>>,
    input: Option<Input>,
}

#[derive(Deserialize)]
struct Input {
    /// `Value` y no texto: otra herramienta podría usar `command` con otra forma.
    command: Option<Value>,
}

/// Comandos de un historial desde `since` (segundos Unix), leídos de a una línea.
pub fn parse_reader(reader: impl BufRead, since: i64) -> Vec<AuditRecord> {
    let mut out = Vec::new();
    let mut shared = Interner::default();
    let mut ai_titles: HashMap<Arc<str>, String> = HashMap::new();
    let mut custom_titles: HashMap<Arc<str>, String> = HashMap::new();
    crate::for_each_line(reader, |raw| {
        // Atajo: casi ninguna línea trae una llamada a herramienta o un título.
        let command_line = raw.contains("\"tool_use\"") && raw.contains("\"assistant\"");
        if !command_line && !raw.contains("-title\"") {
            return;
        }
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            return;
        };
        match line.kind.as_deref() {
            Some("ai-title") => {
                if let (Some(s), Some(t)) = (line.session_id, line.ai_title) {
                    ai_titles.insert(shared.get(&s), t);
                }
            }
            Some("custom-title") => {
                if let (Some(s), Some(t)) = (line.session_id, line.custom_title) {
                    custom_titles.insert(shared.get(&s), t);
                }
            }
            Some("assistant") => {
                let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
                    return;
                };
                if ts < since {
                    return;
                }
                let Some(blocks) = line.message.and_then(|m| m.content) else {
                    return;
                };
                let session = shared.get(line.session_id.as_deref().unwrap_or(""));
                let folder = line.cwd.as_deref().map(|c| shared.get(c));
                for block in blocks {
                    if block.kind.as_deref() != Some("tool_use") {
                        continue;
                    }
                    let Some(tool) = block
                        .name
                        .as_deref()
                        .and_then(|n| SHELL_TOOLS.iter().copied().find(|t| *t == n))
                    else {
                        continue;
                    };
                    let Some(command) = block
                        .input
                        .and_then(|i| i.command)
                        .and_then(|c| c.as_str().map(str::to_string))
                        .filter(|c| !c.trim().is_empty())
                    else {
                        continue;
                    };
                    out.push(AuditRecord {
                        ts,
                        agent: AgentKind::ClaudeCode,
                        session: session.clone(),
                        chat: None,
                        host: LOCAL_HOST,
                        folder: folder.clone(),
                        command: command.into_boxed_str(),
                        tool,
                        key: block.id.map(|k| k.into_owned().into_boxed_str()),
                    });
                }
            }
            _ => {}
        }
    });
    // El título puede aparecer después de los comandos: se asigna al final.
    for r in &mut out {
        r.chat = custom_titles
            .get(&r.session)
            .or_else(|| ai_titles.get(&r.session))
            .map(|t| shared.get(t));
    }
    out.shrink_to_fit();
    out
}

/// Comandos de un texto ya leído (pruebas).
pub fn parse_lines(text: &str) -> Vec<AuditRecord> {
    parse_reader(text.as_bytes(), i64::MIN)
}

/// Comandos de un archivo desde `since` (vacío si no se puede leer).
pub fn read_file(path: &Path, since: i64) -> Vec<AuditRecord> {
    std::fs::File::open(path)
        .map(|f| parse_reader(std::io::BufReader::with_capacity(64 * 1024, f), since))
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
        assert_eq!(&*r[0].command, "ls -la");
        assert_eq!(r[0].tool, "Bash");
        assert_eq!(r[0].agent, AgentKind::ClaudeCode);
        assert_eq!(&*r[0].session, "s1");
        assert_eq!(r[0].host, "local");
        assert_eq!(r[0].folder.as_deref(), Some(r"C:\proyectos\demo"));
        assert_eq!(r[0].key.as_deref(), Some("toolu_1"));
        assert_eq!(r[0].ts, crate::parse_iso("2026-11-20T10:00:05Z").unwrap());
        assert_eq!(&*r[1].command, "Get-ChildItem");
        assert_eq!(r[1].tool, "PowerShell");
    }

    #[test]
    fn sesion_reanudada_no_repite() {
        let mut both = parse_lines(SAMPLE);
        both.extend(parse_lines(SAMPLE));
        let both = both.into_iter().map(Arc::new).collect();
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
        let total: usize = found.iter().map(|p| read_file(p, i64::MIN).len()).sum();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found.len(), 2);
        assert_eq!(total, 4);
        assert!(read_file(Path::new("no-existe.jsonl"), i64::MIN).is_empty());
    }

    #[test]
    fn titulo_del_chat_el_del_usuario_gana() {
        let text = [
            SAMPLE,
            r#"{"type":"ai-title","sessionId":"s1","aiTitle":"Titulo generado"}"#,
            "\n",
        ]
        .concat();
        let r = parse_lines(&text);
        assert!(r.iter().all(|x| x.chat.as_deref() == Some("Titulo generado")));
        let text = [
            &text,
            r#"{"type":"custom-title","sessionId":"s1","customTitle":"Mi chat"}"#,
        ]
        .concat();
        let r = parse_lines(&text);
        assert!(r.iter().all(|x| x.chat.as_deref() == Some("Mi chat")));
        // Sin título: no se inventa uno.
        assert!(parse_lines(SAMPLE).iter().all(|x| x.chat.is_none()));
    }

    #[test]
    fn solo_desde_el_inicio_de_la_ventana() {
        let since = crate::parse_iso("2026-11-20T10:00:30Z").unwrap();
        let r = parse_reader(SAMPLE.as_bytes(), since);
        assert_eq!(r.len(), 1);
        assert_eq!(&*r[0].command, "Get-ChildItem");
    }

    #[test]
    fn comparte_sesion_y_carpeta_entre_comandos() {
        let r = parse_lines(SAMPLE);
        assert!(Arc::ptr_eq(&r[0].session, &r[1].session));
        assert!(Arc::ptr_eq(
            r[0].folder.as_ref().unwrap(),
            r[1].folder.as_ref().unwrap()
        ));
    }
}

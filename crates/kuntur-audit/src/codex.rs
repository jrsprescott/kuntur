//! Comandos de Codex: rollouts `~/.codex/sessions/AAAA/MM/DD/rollout-*.jsonl` (o `.jsonl.zst`).
//!
//! Campos usados (confirmados con archivos reales imprimiendo solo claves, 2026-11):
//! - Cada línea: `{timestamp (ISO 8601), type, payload}`.
//! - `type:"session_meta"` → `payload.{id, cwd}`; `type:"turn_context"` → `payload.cwd`.
//! - `type:"response_item"` con `payload.type:"function_call"` → `payload.{name, arguments, call_id}`.
//!   `name:"shell_command"` y `arguments` es **texto JSON** con `{command (texto), workdir,
//!   timeout_ms, login, justification, sandbox_permissions, prefix_rule}`.
//! - Versiones anteriores de Codex (no presentes en esta PC, se aceptan igual): `name:"shell"` con
//!   `command` como lista (`["bash","-lc","…"]`), `name:"exec_command"` con `cmd`, y
//!   `payload.type:"local_shell_call"` con `action.{command (lista), working_directory}`.
//! - `function_call_output` (`{call_id, output}`) es la salida: no se guarda.
//! - Títulos del chat: `~/.codex/session_index.jsonl` (los lee `kuntur_usage::codex::titles`).
//!
//! Se lee línea por línea (también los `.zst`, descomprimiendo al vuelo).

use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::model::{AuditRecord, Interner, LOCAL_HOST};
use kuntur_usage::AgentKind;

/// Herramientas de Codex que ejecutan comandos.
const SHELL_TOOLS: [&str; 5] = [
    "shell_command",
    "shell",
    "exec_command",
    "container.exec",
    "local_shell",
];

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Value>,
}

/// Una lista `["bash", "-lc", "…"]` → el script; otra lista → sus partes unidas por espacios.
fn command_from_list(parts: &[Value]) -> Option<String> {
    let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
    if parts.len() == 3 {
        let shell = parts[0].rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
        let shell = shell.trim_end_matches(".exe");
        let flag = parts[1].to_ascii_lowercase();
        let wraps = matches!(shell, "bash" | "sh" | "zsh" | "pwsh" | "powershell" | "cmd")
            && matches!(flag.as_str(), "-lc" | "-c" | "-command" | "/c");
        if wraps {
            return Some(parts[2].to_string());
        }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn command_from(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => command_from_list(parts),
        _ => None,
    }
    .filter(|c| !c.trim().is_empty())
}

/// Comandos de un rollout desde `since` (segundos Unix), leídos de a una línea.
/// `fallback_session` identifica la sesión si falta `session_meta` (el nombre del archivo);
/// `titles` son los títulos de los chats por id de sesión.
pub fn parse_reader(
    reader: impl BufRead,
    fallback_session: &str,
    since: i64,
    titles: &HashMap<String, String>,
) -> Vec<AuditRecord> {
    let mut shared = Interner::default();
    let mut session = shared.get(fallback_session);
    let mut cwd: Option<std::sync::Arc<str>> = None;
    let mut out = Vec::new();
    crate::for_each_line(reader, |raw| {
        if !(raw.contains("session_meta")
            || raw.contains("turn_context")
            || raw.contains("function_call\"")
            || raw.contains("local_shell_call"))
        {
            return;
        }
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            return;
        };
        let Some(payload) = line.payload else { return };
        let text_of = |key: &str| payload.get(key).and_then(Value::as_str);
        match line.kind.as_deref() {
            Some("session_meta") => {
                if let Some(id) = text_of("id").or_else(|| text_of("session_id")) {
                    session = shared.get(id);
                }
                if let Some(c) = text_of("cwd") {
                    cwd = Some(shared.get(c));
                }
            }
            Some("turn_context") => {
                if let Some(c) = text_of("cwd") {
                    cwd = Some(shared.get(c));
                }
            }
            Some("response_item") => {
                let Some(ts) = line.timestamp.as_deref().and_then(crate::parse_iso) else {
                    return;
                };
                if ts < since {
                    return;
                }
                let (tool, command, folder) = match payload.get("type").and_then(Value::as_str) {
                    Some("function_call") => {
                        let Some(tool) = text_of("name").and_then(|n| SHELL_TOOLS.iter().copied().find(|t| *t == n))
                        else {
                            return;
                        };
                        let args = match payload.get("arguments") {
                            Some(Value::String(s)) => serde_json::from_str::<Value>(s).unwrap_or(Value::Null),
                            Some(v) => v.clone(),
                            None => Value::Null,
                        };
                        let command = command_from(args.get("command")).or_else(|| command_from(args.get("cmd")));
                        let folder = args.get("workdir").and_then(Value::as_str).map(|f| shared.get(f));
                        (tool, command, folder)
                    }
                    Some("local_shell_call") => {
                        let action = payload.get("action");
                        let command = command_from(action.and_then(|a| a.get("command")));
                        let folder = action
                            .and_then(|a| a.get("working_directory"))
                            .and_then(Value::as_str)
                            .map(|f| shared.get(f));
                        ("local_shell", command, folder)
                    }
                    _ => return,
                };
                let Some(command) = command else { return };
                out.push(AuditRecord {
                    ts,
                    agent: AgentKind::Codex,
                    session: session.clone(),
                    chat: None,
                    host: LOCAL_HOST,
                    folder: folder.or_else(|| cwd.clone()),
                    command: command.into_boxed_str(),
                    tool,
                    key: text_of("call_id").map(Into::into),
                });
            }
            _ => {}
        }
    });
    for r in &mut out {
        r.chat = titles.get(&*r.session).map(|t| shared.get(t));
    }
    out.shrink_to_fit();
    out
}

/// Comandos de un texto ya descomprimido (pruebas).
pub fn parse_lines(text: &str, fallback_session: &str) -> Vec<AuditRecord> {
    parse_reader(text.as_bytes(), fallback_session, i64::MIN, &HashMap::new())
}

/// Nombre del archivo sin `.jsonl` / `.jsonl.zst`.
fn file_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.trim_end_matches(".zst").trim_end_matches(".jsonl").to_string()
}

/// Comandos de un rollout desde `since` (descomprime `.zst` al vuelo); vacío si no se puede leer.
pub fn read_file(path: &Path, since: i64, titles: &HashMap<String, String>) -> Vec<AuditRecord> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let stem = file_stem(path);
    if path.extension().is_some_and(|e| e == "zst") {
        match zstd::stream::read::Decoder::new(file) {
            Ok(dec) => parse_reader(std::io::BufReader::new(dec), &stem, since, titles),
            Err(_) => Vec::new(),
        }
    } else {
        parse_reader(std::io::BufReader::with_capacity(64 * 1024, file), &stem, since, titles)
    }
}

/// Rollouts bajo `root`.
pub fn files(root: &Path) -> Vec<std::path::PathBuf> {
    crate::files_under(root, &[".jsonl", ".jsonl.zst"])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"timestamp":"2026-11-20T10:00:00.000Z","type":"session_meta","payload":{"id":"ses-1","cwd":"C:\\proyectos\\api"}}
{"timestamp":"2026-11-20T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-5.5","cwd":"C:\\proyectos\\api"}}
{"timestamp":"2026-11-20T10:00:02.000Z","type":"response_item","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"cargo test\",\"workdir\":\"C:\\\\proyectos\\\\api\\\\crate\",\"timeout_ms\":1000}","call_id":"call_1"}}
{"timestamp":"2026-11-20T10:00:03.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"ok"}}
{"timestamp":"2026-11-20T10:00:04.000Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"bash\",\"-lc\",\"ls -la\"]}","call_id":"call_2"}}
{"timestamp":"2026-11-20T10:00:05.000Z","type":"response_item","payload":{"type":"local_shell_call","call_id":"call_3","action":{"type":"exec","command":["git","status"],"working_directory":"/srv/demo"}}}
{"timestamp":"2026-11-20T10:00:06.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"npm run build\"}","call_id":"call_4"}}
{"timestamp":"2026-11-20T10:00:07.000Z","type":"response_item","payload":{"type":"function_call","name":"apply_patch","arguments":"{\"input\":\"x\"}","call_id":"call_5"}}
{"timestamp":"2026-11-20T10:00:08.000Z","type":"response_item","payload":{"type":"function_call","name":"shell_command","arguments":"no es json","call_id":"call_6"}}
"#;

    #[test]
    fn lee_comandos_de_codex() {
        let r = parse_lines(SAMPLE, "rollout-x");
        let commands: Vec<&str> = r.iter().map(|r| &*r.command).collect();
        assert_eq!(commands, ["cargo test", "ls -la", "git status", "npm run build"]);
        assert!(
            r.iter()
                .all(|r| r.agent == AgentKind::Codex && &*r.session == "ses-1" && r.host == "local")
        );
        assert_eq!(r[0].folder.as_deref(), Some(r"C:\proyectos\api\crate"));
        assert_eq!(r[0].tool, "shell_command");
        assert_eq!(r[0].key.as_deref(), Some("call_1"));
        // Sin `workdir`: la carpeta del turno.
        assert_eq!(r[1].folder.as_deref(), Some(r"C:\proyectos\api"));
        assert_eq!(r[2].folder.as_deref(), Some("/srv/demo"));
        assert_eq!(r[2].tool, "local_shell");
        assert_eq!(r[3].ts, crate::parse_iso("2026-11-20T10:00:06Z").unwrap());
    }

    #[test]
    fn sin_session_meta_usa_el_nombre_del_archivo() {
        let only_call = SAMPLE.lines().nth(2).unwrap();
        let r = parse_lines(only_call, "rollout-x");
        assert_eq!(r.len(), 1);
        assert_eq!(&*r[0].session, "rollout-x");
        assert_eq!(r[0].folder.as_deref(), Some(r"C:\proyectos\api\crate"));
    }

    #[test]
    fn listas_de_comando() {
        let v = |a: &[&str]| a.iter().map(|s| Value::from(*s)).collect::<Vec<_>>();
        assert_eq!(
            command_from_list(&v(&["bash", "-lc", "echo hola"])).as_deref(),
            Some("echo hola")
        );
        assert_eq!(
            command_from_list(&v(&[r"C:\Program Files\PowerShell\7\pwsh.exe", "-Command", "dir"])).as_deref(),
            Some("dir")
        );
        assert_eq!(
            command_from_list(&v(&["git", "log", "-1"])).as_deref(),
            Some("git log -1")
        );
        assert_eq!(command_from_list(&[]), None);
    }

    #[test]
    fn lee_rollouts_comprimidos() {
        let root = std::env::temp_dir().join(format!("kuntur-audit-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let day = root.join("2026").join("11").join("20");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-a.jsonl"), SAMPLE).unwrap();
        let packed = zstd::encode_all(SAMPLE.as_bytes(), 3).unwrap();
        std::fs::write(day.join("rollout-b.jsonl.zst"), packed).unwrap();
        std::fs::write(day.join("rollout-c.jsonl.zst"), b"no es zstd").unwrap();
        let found = files(&root);
        let counts: Vec<usize> = found
            .iter()
            .map(|p| read_file(p, i64::MIN, &HashMap::new()).len())
            .collect();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(counts, [4, 4, 0]);
    }

    #[test]
    fn titulo_del_chat_desde_el_indice() {
        let titles = HashMap::from([("ses-1".to_string(), "Arreglar la API".to_string())]);
        let r = parse_reader(SAMPLE.as_bytes(), "rollout-x", i64::MIN, &titles);
        assert!(r.iter().all(|x| x.chat.as_deref() == Some("Arreglar la API")));
        let since = crate::parse_iso("2026-11-20T10:00:05Z").unwrap();
        assert_eq!(parse_reader(SAMPLE.as_bytes(), "rollout-x", since, &titles).len(), 2);
    }
}

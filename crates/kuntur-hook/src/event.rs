//! Evento compacto que viaja del gancho a Kuntur por el named pipe, y su conversión desde la entrada
//! de los hooks y de la línea de estado de Claude Code (JSON por stdin).
//!
//! Campos de Claude Code (documentación oficial de hooks, 2026-11-23): comunes `session_id`, `cwd`,
//! `hook_event_name` y, dentro de un subagente, `agent_id` / `agent_type`; `PreToolUse` /
//! `PostToolUse` (`tool_name`, `tool_input`, `tool_use_id`); `PermissionRequest`; `Notification`
//! (`message`, `notification_type`); `Stop` / `SubagentStop` (`last_assistant_message`);
//! `SubagentStart`. La línea de estado trae `rate_limits.five_hour` / `seven_day`
//! (`used_percentage`, `resets_at`) y `model.display_name`, `context_window.used_percentage`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Largo máximo del último mensaje del agente que se reenvía (para extraer comandos sugeridos).
pub const MAX_MESSAGE: usize = 16 * 1024;
/// Largo máximo del resumen de una herramienta ("Bash · npm test").
pub const MAX_DETAIL: usize = 160;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    SessionStart,
    SessionEnd,
    /// El usuario envió un mensaje (`UserPromptSubmit`).
    #[default]
    Prompt,
    /// Empieza una herramienta (`PreToolUse`).
    PreTool,
    /// Terminó una herramienta (`PostToolUse` / `PostToolUseFailure`).
    PostTool,
    /// Pide permiso para una herramienta (`PermissionRequest`).
    Permission,
    Notification,
    /// Terminó su turno (`Stop`).
    Stop,
    SubagentStart,
    SubagentStop,
    /// Datos de la línea de estado (límites de uso).
    StatusLine,
}

/// Límites de uso de Claude que llegan por la línea de estado.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<i64>,
}

impl StatusLimits {
    pub fn is_empty(&self) -> bool {
        self.five_hour_pct.is_none() && self.seven_day_pct.is_none()
    }
}

/// Un evento de un agente en una terminal de Kuntur.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HookEvent {
    /// Terminal de Kuntur (`KUNTUR_SESSION`).
    pub pane: String,
    pub kind: EventKind,
    /// Microsegundos Unix en que se generó. El pipe atiende varios ganchos a la vez y puede
    /// entregarlos desordenados: Kuntur descarta un evento más viejo que el último aplicado.
    #[serde(default)]
    pub ts: u64,
    /// Id del agente en el catálogo de Kuntur (`claude`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Subagente que generó el evento (ausente si es el agente principal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Resumen de la entrada de la herramienta (comando, archivo, patrón…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// `notification_type` de `Notification` (`permission_prompt`, `idle_prompt`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification: Option<String>,
    /// Último mensaje del agente (`Stop`), recortado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<StatusLimits>,
}

/// Microsegundos Unix de ahora.
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

impl HookEvent {
    /// Una línea JSON terminada en `\n`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec(self).unwrap_or_default();
        out.push(b'\n');
        out
    }
}

/// Eventos de un mensaje del pipe (una línea JSON por evento; las inválidas se ignoran).
pub fn decode(bytes: &[u8]) -> Vec<HookEvent> {
    bytes
        .split(|&b| b == b'\n')
        .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
        .filter_map(|l| serde_json::from_slice(l).ok())
        .collect()
}

/// Recorta a `max` bytes sin partir un carácter; agrega "…" si cortó.
pub fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Primera línea no vacía, recortada.
fn first_line(s: &str) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let more = s.trim().lines().count() > 1;
    let line = clip(line, MAX_DETAIL);
    if more && !line.ends_with('…') {
        format!("{line} …")
    } else {
        line
    }
}

/// Nombre de archivo de una ruta (`C:\a\b.rs` o `/a/b.rs` → `b.rs`).
fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .find(|p| !p.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Resumen de lo que hace una herramienta: el comando de Bash, el archivo de Read/Edit, el patrón de
/// Grep, la URL de WebFetch… `None` si no hay nada útil que mostrar.
pub fn tool_detail(tool: &str, input: &Value) -> Option<String> {
    let s = |k: &str| text(input, k);
    let detail = match tool {
        "Bash" | "PowerShell" => s("command").map(|c| first_line(&c)),
        "Read" | "Write" | "Edit" | "MultiEdit" => s("file_path").map(|p| file_name(&p)),
        "NotebookEdit" | "NotebookRead" => s("notebook_path").map(|p| file_name(&p)),
        "Grep" => s("pattern").map(|p| format!("\"{}\"", clip(&p, MAX_DETAIL - 2))),
        "Glob" => s("pattern"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query").map(|q| format!("\"{}\"", clip(&q, MAX_DETAIL - 2))),
        "Agent" | "Task" => s("description").or_else(|| s("subagent_type")),
        "Skill" => s("skill"),
        _ => s("command")
            .or_else(|| s("file_path").map(|p| file_name(&p)))
            .or_else(|| s("description")),
    };
    detail.map(|d| clip(&d, MAX_DETAIL)).filter(|d| !d.is_empty())
}

/// Convierte la entrada de un hook de Claude Code. `None` para eventos que Kuntur no usa.
pub fn from_claude(input: &Value, pane: &str) -> Option<HookEvent> {
    let kind = match input.get("hook_event_name")?.as_str()? {
        "SessionStart" => EventKind::SessionStart,
        "SessionEnd" => EventKind::SessionEnd,
        "UserPromptSubmit" => EventKind::Prompt,
        "PreToolUse" => EventKind::PreTool,
        "PostToolUse" | "PostToolUseFailure" => EventKind::PostTool,
        "PermissionRequest" => EventKind::Permission,
        "Notification" => EventKind::Notification,
        "Stop" => EventKind::Stop,
        "SubagentStart" => EventKind::SubagentStart,
        "SubagentStop" => EventKind::SubagentStop,
        _ => return None,
    };
    let tool = text(input, "tool_name");
    let detail = tool
        .as_deref()
        .and_then(|t| tool_detail(t, input.get("tool_input").unwrap_or(&Value::Null)));
    let message = match kind {
        EventKind::Stop => text(input, "last_assistant_message").map(|m| clip(&m, MAX_MESSAGE)),
        EventKind::Notification => text(input, "message").map(|m| clip(&m, MAX_DETAIL)),
        _ => None,
    };
    Some(HookEvent {
        pane: pane.to_string(),
        kind,
        ts: now_micros(),
        agent: Some("claude".into()),
        agent_id: text(input, "agent_id"),
        agent_type: text(input, "agent_type"),
        tool,
        detail,
        notification: text(input, "notification_type"),
        message,
        limits: None,
    })
}

/// Límites de la entrada de la línea de estado (`None` si no trae ninguno).
pub fn limits_from_statusline(input: &Value) -> Option<StatusLimits> {
    let rl = input.get("rate_limits")?;
    let pct = |w: &str| rl.get(w)?.get("used_percentage")?.as_f64().filter(|p| p.is_finite());
    let resets = |w: &str| rl.get(w)?.get("resets_at")?.as_i64();
    let limits = StatusLimits {
        five_hour_pct: pct("five_hour"),
        five_hour_resets_at: resets("five_hour"),
        seven_day_pct: pct("seven_day"),
        seven_day_resets_at: resets("seven_day"),
    };
    (!limits.is_empty()).then_some(limits)
}

/// Evento de la línea de estado (`None` si no trae límites: no hay nada que avisar).
pub fn from_statusline(input: &Value, pane: &str) -> Option<HookEvent> {
    Some(HookEvent {
        pane: pane.to_string(),
        kind: EventKind::StatusLine,
        ts: now_micros(),
        agent: Some("claude".into()),
        limits: Some(limits_from_statusline(input)?),
        ..HookEvent::default()
    })
}

/// Texto corto que se muestra en la línea de estado de Claude Code: "Opus · contexto 23 % · 5 h 40 %".
pub fn statusline_text(input: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(model) = input.get("model").and_then(|m| text(m, "display_name")) {
        parts.push(model);
    }
    if let Some(ctx) = input
        .get("context_window")
        .and_then(|c| c.get("used_percentage"))
        .and_then(Value::as_f64)
    {
        parts.push(format!("contexto {} %", ctx.round() as i64));
    }
    if let Some(l) = limits_from_statusline(input) {
        if let Some(p) = l.five_hour_pct {
            parts.push(format!("5 h {} %", p.round() as i64));
        }
        if let Some(p) = l.seven_day_pct {
            parts.push(format!("semanal {} %", p.round() as i64));
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pre_tool_use_de_bash_con_resumen() {
        let input = json!({
            "session_id": "abc", "cwd": "C:\\x", "hook_event_name": "PreToolUse",
            "tool_name": "Bash", "tool_input": {"command": "npm test\nnpm run lint", "description": "Pruebas"},
            "tool_use_id": "toolu_1"
        });
        let e = from_claude(&input, "k3").unwrap();
        assert_eq!(e.kind, EventKind::PreTool);
        assert_eq!(e.pane, "k3");
        assert_eq!(e.agent.as_deref(), Some("claude"));
        assert_eq!(e.tool.as_deref(), Some("Bash"));
        assert_eq!(e.detail.as_deref(), Some("npm test …"));
        assert_eq!(e.agent_id, None);
    }

    #[test]
    fn subagentes_y_resumenes_por_herramienta() {
        let e = from_claude(
            &json!({"hook_event_name": "PreToolUse", "agent_id": "a1", "agent_type": "Explore",
                    "tool_name": "Grep", "tool_input": {"pattern": "fn main"}}),
            "k",
        )
        .unwrap();
        assert_eq!(e.agent_id.as_deref(), Some("a1"));
        assert_eq!(e.agent_type.as_deref(), Some("Explore"));
        assert_eq!(e.detail.as_deref(), Some("\"fn main\""));
        let read = json!({"file_path": "C:\\repo\\src\\main.rs"});
        assert_eq!(tool_detail("Read", &read).as_deref(), Some("main.rs"));
        assert_eq!(
            tool_detail("Edit", &json!({"file_path": "/home/x/lib.rs"})).as_deref(),
            Some("lib.rs")
        );
        assert_eq!(
            tool_detail("WebFetch", &json!({"url": "https://example.com"})).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(tool_detail("TodoWrite", &json!({"todos": []})), None);
        let long = "x".repeat(500);
        assert!(tool_detail("Bash", &json!({ "command": long })).unwrap().ends_with('…'));
    }

    #[test]
    fn notificaciones_stop_y_eventos_ignorados() {
        let n = from_claude(
            &json!({"hook_event_name": "Notification", "message": "Claude needs your permission",
                    "notification_type": "permission_prompt"}),
            "k",
        )
        .unwrap();
        assert_eq!(n.kind, EventKind::Notification);
        assert_eq!(n.notification.as_deref(), Some("permission_prompt"));
        let s = from_claude(
            &json!({"hook_event_name": "Stop", "stop_hook_active": false,
                    "last_assistant_message": "Listo. Ejecuta:\n```bash\nnpm test\n```"}),
            "k",
        )
        .unwrap();
        assert_eq!(s.kind, EventKind::Stop);
        assert!(s.message.unwrap().contains("npm test"));
        let big = "é".repeat(MAX_MESSAGE);
        let s = from_claude(&json!({"hook_event_name": "Stop", "last_assistant_message": big}), "k").unwrap();
        assert!(s.message.unwrap().len() <= MAX_MESSAGE + 3);
        assert!(from_claude(&json!({"hook_event_name": "PreCompact"}), "k").is_none());
        assert!(from_claude(&json!({"otra": 1}), "k").is_none());
        let p = from_claude(
            &json!({"hook_event_name": "PostToolUseFailure", "tool_name": "Bash",
                    "tool_input": {"command": "cargo build"}, "error": "x"}),
            "k",
        )
        .unwrap();
        assert_eq!(p.kind, EventKind::PostTool);
    }

    #[test]
    fn codifica_y_decodifica_varias_lineas() {
        let a = from_claude(&json!({"hook_event_name": "UserPromptSubmit", "prompt": "hola"}), "k1").unwrap();
        let b = from_claude(
            &json!({"hook_event_name": "SubagentStart", "agent_id": "x", "agent_type": "Plan"}),
            "k2",
        )
        .unwrap();
        let mut bytes = a.encode();
        bytes.extend(b"basura que no es json\n\n");
        bytes.extend(b.encode());
        assert_eq!(decode(&bytes), vec![a, b]);
        // El texto del usuario no viaja.
        assert!(!String::from_utf8_lossy(&decode(&bytes)[0].encode()).contains("hola"));
    }

    #[test]
    fn linea_de_estado_con_limites() {
        let input = json!({
            "model": {"display_name": "Opus"},
            "context_window": {"used_percentage": 23.4},
            "rate_limits": {
                "five_hour": {"used_percentage": 40.2, "resets_at": 1738425600},
                "seven_day": {"used_percentage": 12.0, "resets_at": 1738857600}
            }
        });
        let e = from_statusline(&input, "k").unwrap();
        let l = e.limits.unwrap();
        assert_eq!(l.five_hour_pct, Some(40.2));
        assert_eq!(l.seven_day_resets_at, Some(1738857600));
        assert_eq!(
            statusline_text(&input),
            "Opus · contexto 23 % · 5 h 40 % · semanal 12 %"
        );
        // Sin rate_limits (API key, o antes de la primera respuesta): no hay evento.
        let sin = json!({"model": {"display_name": "Sonnet"}});
        assert!(from_statusline(&sin, "k").is_none());
        assert_eq!(statusline_text(&sin), "Sonnet");
    }

    #[test]
    fn recorta_sin_partir_caracteres() {
        assert_eq!(clip("hola", 10), "hola");
        assert_eq!(clip("ñandú", 3), "ña…");
        assert_eq!(clip("ñandú", 1), "…");
    }
}

//! Registro de auditoría: un comando que ejecutó un agente.

use std::collections::HashSet;

use kuntur_usage::AgentKind;

/// Host de los registros leídos en esta PC (F6 sumará las sesiones remotas).
pub const LOCAL_HOST: &str = "local";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// Segundos Unix (UTC).
    pub ts: i64,
    pub agent: AgentKind,
    /// Id de la sesión del agente.
    pub session: String,
    /// `local` (o el servidor, cuando lleguen las sesiones remotas).
    pub host: String,
    /// Carpeta de trabajo del comando, si se conoce.
    pub folder: Option<String>,
    pub command: String,
    /// Herramienta del agente (`Bash`, `PowerShell`, `shell_command`…).
    pub tool: String,
    /// Identidad de la llamada (`tool_use.id`, `call_id`) para no contarla dos veces cuando el agente
    /// copia el historial en otro archivo (Claude Code al reanudar una sesión).
    pub key: Option<String>,
}

/// Sin repetidos (misma `key` y agente) y del más nuevo al más viejo.
pub fn dedup(mut records: Vec<AuditRecord>) -> Vec<AuditRecord> {
    records.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.command.cmp(&b.command)));
    let mut seen = HashSet::new();
    records.retain(|r| match &r.key {
        Some(k) => seen.insert((r.agent, k.clone())),
        None => true,
    });
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: i64, key: Option<&str>, agent: AgentKind) -> AuditRecord {
        AuditRecord {
            ts,
            agent,
            session: "s".into(),
            host: LOCAL_HOST.into(),
            folder: None,
            command: format!("echo {ts}"),
            tool: "Bash".into(),
            key: key.map(str::to_string),
        }
    }

    #[test]
    fn quita_repetidos_y_ordena_del_mas_nuevo() {
        let out = dedup(vec![
            rec(10, Some("a"), AgentKind::ClaudeCode),
            rec(30, Some("b"), AgentKind::ClaudeCode),
            rec(10, Some("a"), AgentKind::ClaudeCode),
            // Misma clave en otro agente: es otra llamada.
            rec(20, Some("a"), AgentKind::Codex),
            rec(5, None, AgentKind::Codex),
            rec(5, None, AgentKind::Codex),
        ]);
        let ts: Vec<i64> = out.iter().map(|r| r.ts).collect();
        assert_eq!(ts, [30, 20, 10, 5, 5]);
    }
}

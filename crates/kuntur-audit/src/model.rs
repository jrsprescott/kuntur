//! Registro de auditoría: un comando que ejecutó un agente.

use std::collections::HashSet;
use std::sync::Arc;

use kuntur_usage::AgentKind;

/// Host de los registros leídos en esta PC (F6 sumará las sesiones remotas).
pub const LOCAL_HOST: &str = "local";

/// Un comando. Lo que se repite entre comandos (sesión, título del chat, carpeta) se comparte con
/// `Arc<str>` (un solo texto por archivo leído), y la herramienta y el host son textos fijos: así
/// miles de comandos ocupan poco más que su propio texto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// Segundos Unix (UTC).
    pub ts: i64,
    pub agent: AgentKind,
    /// Id de la sesión (el chat) del agente.
    pub session: Arc<str>,
    /// Título del chat en que se usó el comando: el que puso el usuario o, si no, el que generó el
    /// agente. `None` si el agente no guardó ninguno.
    pub chat: Option<Arc<str>>,
    /// `local` (o el servidor, cuando lleguen las sesiones remotas).
    pub host: &'static str,
    /// Carpeta de trabajo del comando, si se conoce.
    pub folder: Option<Arc<str>>,
    pub command: Box<str>,
    /// Herramienta del agente (`Bash`, `PowerShell`, `shell_command`…).
    pub tool: &'static str,
    /// Identidad de la llamada (`tool_use.id`, `call_id`) para no contarla dos veces cuando el agente
    /// copia el historial en otro archivo (Claude Code al reanudar una sesión).
    pub key: Option<Box<str>>,
}

/// Un solo `Arc<str>` por texto distinto (sesiones, carpetas, títulos de un archivo).
#[derive(Default)]
pub(crate) struct Interner(HashSet<Arc<str>>);

impl Interner {
    pub(crate) fn get(&mut self, text: &str) -> Arc<str> {
        if let Some(found) = self.0.get(text) {
            return found.clone();
        }
        let shared: Arc<str> = Arc::from(text);
        self.0.insert(shared.clone());
        shared
    }
}

/// Sin repetidos (misma `key` y agente) y del más nuevo al más viejo.
pub fn dedup(mut records: Vec<Arc<AuditRecord>>) -> Vec<Arc<AuditRecord>> {
    records.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.command.cmp(&b.command)));
    let mut seen = HashSet::new();
    records.retain(|r| match &r.key {
        Some(k) => seen.insert((r.agent, k.clone())),
        None => true,
    });
    records.shrink_to_fit();
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: i64, key: Option<&str>, agent: AgentKind) -> Arc<AuditRecord> {
        Arc::new(AuditRecord {
            ts,
            agent,
            session: "s".into(),
            chat: None,
            host: LOCAL_HOST,
            folder: None,
            command: format!("echo {ts}").into(),
            tool: "Bash",
            key: key.map(Into::into),
        })
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

    #[test]
    fn el_mismo_texto_se_comparte() {
        let mut i = Interner::default();
        let a = i.get("C:/proyectos/demo");
        let b = i.get("C:/proyectos/demo");
        assert!(Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &i.get("C:/otro")));
    }
}

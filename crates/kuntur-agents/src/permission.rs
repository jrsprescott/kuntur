//! "Permitir / Rechazar" desde la fila de un agente que pide permiso: qué teclas se escriben en su
//! terminal. Solo para agentes cuyo diálogo de permisos se conoce; para los demás no hay botones
//! (se ve "pide permiso" y hay que responder en la terminal). Solo se usa por clic del usuario.

/// Respuesta del usuario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Allow,
    Deny,
}

/// Bytes que responden el diálogo de permiso del agente `agent` (id del catálogo).
/// - Claude Code: `1` elige "Sí" (primera opción); Esc es "No, y decirle qué hacer distinto".
/// - Codex: `y` aprueba; Esc cancela.
pub fn answer_keys(agent: &str, answer: Answer) -> Option<&'static [u8]> {
    match (agent, answer) {
        ("claude", Answer::Allow) => Some(b"1"),
        ("codex", Answer::Allow) => Some(b"y"),
        ("claude" | "codex", Answer::Deny) => Some(b"\x1b"),
        _ => None,
    }
}

/// ¿Se pueden mostrar los botones para este agente?
pub fn can_answer(agent: &str) -> bool {
    answer_keys(agent, Answer::Allow).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teclas_por_agente() {
        assert_eq!(answer_keys("claude", Answer::Allow), Some(&b"1"[..]));
        assert_eq!(answer_keys("claude", Answer::Deny), Some(&b"\x1b"[..]));
        assert_eq!(answer_keys("codex", Answer::Allow), Some(&b"y"[..]));
        assert_eq!(answer_keys("codex", Answer::Deny), Some(&b"\x1b"[..]));
        assert_eq!(answer_keys("opencode", Answer::Allow), None);
        assert_eq!(answer_keys("extra:aider", Answer::Deny), None);
        assert!(can_answer("claude"));
        assert!(!can_answer("agente"));
    }
}

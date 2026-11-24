//! OSC 7501: estado de un agente que el propio agente (o un gancho) escribe en su terminal. Formato
//! de Kuntur (no hay un estándar confirmado):
//!
//! `ESC ] 7501 ; state=<estado> [; agent=<id>] [; action=<texto>] (BEL | ESC \)`
//!
//! `<estado>`: `working` (`trabajando`), `permission` (`permiso`), `waiting` (`esperando`, espera tu
//! respuesta), `done` (`listo`, `terminado`) o `end` (`fin`, el agente se cerró). Forma corta:
//! `ESC ] 7501 ; working BEL`. Los valores no pueden contener `;`. Sirve de respaldo de los hooks y
//! como única fuente para agentes sin hooks (y para los remotos por SSH).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalState {
    Working,
    Permission,
    Waiting,
    Done,
    End,
}

impl SignalState {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "working" | "trabajando" | "busy" => Self::Working,
            "permission" | "permiso" => Self::Permission,
            "waiting" | "esperando" | "input" => Self::Waiting,
            "done" | "listo" | "terminado" | "idle" => Self::Done,
            "end" | "fin" | "exit" => Self::End,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSignal {
    pub state: SignalState,
    /// Id del agente en el catálogo de Kuntur (`claude`, `codex`…), si lo dice.
    pub agent: Option<String>,
    /// Acción actual ("Bash · npm test").
    pub action: Option<String>,
}

/// Largo máximo de los textos de la señal.
const MAX_FIELD: usize = 160;

fn clean(v: &str) -> Option<String> {
    let v: String = v.chars().filter(|c| !c.is_control()).take(MAX_FIELD).collect();
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Señal de un OSC (contenido sin `ESC ]` ni terminador); `None` si no es un OSC 7501 válido.
pub fn parse_7501(payload: &[u8]) -> Option<AgentSignal> {
    let text = String::from_utf8_lossy(payload);
    let rest = text.strip_prefix("7501;")?;
    let mut state = None;
    let (mut agent, mut action) = (None, None);
    for (i, field) in rest.split(';').enumerate() {
        match field.split_once('=') {
            Some((k, v)) => match k.trim().to_ascii_lowercase().as_str() {
                "state" | "estado" => state = SignalState::parse(v),
                "agent" | "agente" => agent = clean(v).map(|a| a.to_ascii_lowercase()),
                "action" | "accion" | "acción" => action = clean(v),
                _ => {}
            },
            None if i == 0 => state = SignalState::parse(field),
            None => {}
        }
    }
    Some(AgentSignal {
        state: state?,
        agent,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::OscScanner;

    fn scan(bytes: &[u8]) -> Vec<AgentSignal> {
        let mut out = Vec::new();
        OscScanner::new().feed(bytes, |p| out.extend(parse_7501(p)));
        out
    }

    #[test]
    fn forma_completa_y_corta() {
        let s = scan(b"x\x1b]7501;state=working;agent=Claude;action=Bash \xc2\xb7 npm test\x07y");
        assert_eq!(
            s,
            vec![AgentSignal {
                state: SignalState::Working,
                agent: Some("claude".into()),
                action: Some("Bash · npm test".into()),
            }]
        );
        let s = scan(b"\x1b]7501;permission\x1b\\");
        assert_eq!(s[0].state, SignalState::Permission);
        assert_eq!(s[0].agent, None);
        assert_eq!(scan(b"\x1b]7501;estado=listo\x07")[0].state, SignalState::Done);
        assert_eq!(scan(b"\x1b]7501;esperando\x07")[0].state, SignalState::Waiting);
        assert_eq!(scan(b"\x1b]7501;state=fin\x07")[0].state, SignalState::End);
    }

    #[test]
    fn invalidos_y_otros_osc() {
        assert!(scan(b"\x1b]7501;state=bailando\x07").is_empty());
        assert!(scan(b"\x1b]7501;\x07").is_empty());
        assert!(scan(b"\x1b]75010;working\x07").is_empty());
        assert!(scan(b"\x1b]9;9;C:\\x\x07\x1b]0;titulo\x07").is_empty());
        // Partida entre lecturas.
        let mut sc = OscScanner::new();
        let mut got = Vec::new();
        for chunk in [&b"\x1b]75"[..], b"01;state=do", b"ne\x1b", b"\\"] {
            sc.feed(chunk, |p| got.extend(parse_7501(p)));
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].state, SignalState::Done);
    }

    #[test]
    fn recorta_textos_y_quita_controles() {
        let long = format!("\x1b]7501;working;action={}\x07", "a".repeat(400));
        assert_eq!(scan(long.as_bytes())[0].action.as_ref().unwrap().len(), MAX_FIELD);
        let s = scan(b"\x1b]7501;working;action=\x01\x02  \x07");
        assert_eq!(s[0].action, None);
    }
}
